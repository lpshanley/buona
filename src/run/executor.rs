//! Command execution helpers for `buona run`.

use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::styles::Styles;

use super::RunOptions;
use super::error::RunError;
use super::hooks::HookResolution;
use super::ops::EffectiveExecution;
use super::output;
use super::planner::{TargetRunPlan, resolve_target_run_plan};
use super::targets::{ExecutionTarget, list_workspace_package_targets};
use super::types::{ExecutionPlan, FailPolicy, ResolvedHook};

/// How child process output reaches the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    /// Child inherits our stdio directly — full TTY behavior (colors,
    /// progress bars, interactive prompts). Used when exactly one target
    /// runs serially, so nothing can interleave.
    Inherit,
    /// Child output is captured and re-printed line-by-line with a
    /// `[target:…/stage]` prefix. Used whenever output from multiple
    /// targets or stages could interleave.
    Streamed,
}

pub(super) async fn execute_recursive(
    options: &RunOptions,
    ws_root: &std::path::Path,
    exec: EffectiveExecution,
) -> Result<()> {
    let s = Styles::default();
    let pkg_targets = list_workspace_package_targets(ws_root).await?;
    let ws_target = ExecutionTarget {
        name: "root".to_string(),
        dir: ws_root.to_path_buf(),
        is_workspace_root: true,
    };

    let ws_plan =
        resolve_target_run_plan(ws_target, &options.command, &options.args, options.system).await?;

    let mut pkg_plans = Vec::new();
    for target in pkg_targets {
        pkg_plans.push(
            resolve_target_run_plan(target, &options.command, &options.args, options.system)
                .await?,
        );
    }

    if options.dry_run {
        output::print_recursive_graph_header(&s);
        output::print_dry_run_stage(
            &s,
            "workspace/pre",
            ws_plan.hooks.pre_hook.as_ref().map(|x| x.display.as_str()),
        );
        output::print_dry_run_command_stage(&s, "workspace/cmd", &ws_plan.plan, options.verbose);
        println!(
            "  {} {} (parallel: {}, jobs: {}, fail-policy: {})",
            s.dim.apply_to("·"),
            s.bold.apply_to("workspace/pkg-exec"),
            if exec.parallel { "on" } else { "off" },
            exec.jobs,
            exec.fail_policy,
        );

        for pkg in &pkg_plans {
            let label = pkg.target.label();
            output::print_dry_run_stage(
                &s,
                &format!("pkg:{label}/pre"),
                pkg.hooks.pre_hook.as_ref().map(|x| x.display.as_str()),
            );
            output::print_dry_run_command_stage(
                &s,
                &format!("pkg:{label}/cmd"),
                &pkg.plan,
                options.verbose,
            );
            output::print_dry_run_stage(
                &s,
                &format!("pkg:{label}/post"),
                pkg.hooks.post_hook.as_ref().map(|x| x.display.as_str()),
            );
        }

        output::print_dry_run_stage(
            &s,
            "workspace/post",
            ws_plan.hooks.post_hook.as_ref().map(|x| x.display.as_str()),
        );
        println!();
        return Ok(());
    }

    let root_label = "root";
    if let Some(pre) = ws_plan.hooks.pre_hook.as_ref() {
        execute_hook(pre, root_label, OutputMode::Streamed).await?;
    }
    execute_plan(&ws_plan.plan, root_label, OutputMode::Streamed).await?;

    execute_package_stage(pkg_plans, exec, OutputMode::Streamed).await?;

    if let Some(post) = ws_plan.hooks.post_hook.as_ref() {
        execute_hook(post, root_label, OutputMode::Streamed).await?;
    }

    Ok(())
}

pub(super) async fn execute_target_plans(
    target_plans: Vec<TargetRunPlan>,
    exec: EffectiveExecution,
) -> Result<()> {
    // A single serial target gets the terminal to itself, so the child can
    // run with full TTY behavior instead of prefixed line streaming.
    let mode = if !exec.parallel && target_plans.len() == 1 {
        OutputMode::Inherit
    } else {
        OutputMode::Streamed
    };
    execute_package_stage(target_plans, exec, mode).await
}

async fn execute_package_stage(
    pkg_plans: Vec<TargetRunPlan>,
    exec: EffectiveExecution,
    mode: OutputMode,
) -> Result<()> {
    if !exec.parallel || exec.jobs <= 1 || pkg_plans.len() <= 1 {
        for pkg in pkg_plans {
            let label = pkg.target.label();
            execute_with_hooks(&pkg.plan, &pkg.hooks, &label, mode).await?;
        }
        return Ok(());
    }

    let semaphore = Arc::new(Semaphore::new(exec.jobs));
    let mut set = JoinSet::new();

    for pkg in pkg_plans {
        let label = pkg.target.label();
        let sem = semaphore.clone();
        set.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| RunError::ConfigError("parallel scheduler closed".to_string()))?;
            let result =
                execute_with_hooks(&pkg.plan, &pkg.hooks, &label, OutputMode::Streamed).await;
            Ok::<Result<()>, RunError>(result)
        });
    }

    let mut first_error: Option<anyhow::Error> = None;
    let mut record_error = |err: anyhow::Error| {
        if first_error.is_none() {
            first_error = Some(err);
        }
    };

    while let Some(joined) = set.join_next().await {
        let failed = match joined {
            Ok(Ok(Ok(()))) => false,
            Ok(Ok(Err(err))) => {
                record_error(err);
                true
            }
            Ok(Err(err)) => {
                record_error(err.into());
                true
            }
            Err(join_err) => {
                record_error(anyhow::anyhow!("parallel task join error: {join_err}"));
                true
            }
        };

        if failed && exec.fail_policy == FailPolicy::FailFast {
            set.abort_all();
            break;
        }
    }

    while set.join_next().await.is_some() {}

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(())
}

async fn execute_with_hooks(
    plan: &ExecutionPlan,
    hooks: &HookResolution,
    target_label: &str,
    mode: OutputMode,
) -> Result<()> {
    if let Some(pre_hook) = hooks.pre_hook.as_ref() {
        execute_hook(pre_hook, target_label, mode).await?;
    }

    execute_plan(plan, target_label, mode).await?;

    if let Some(post_hook) = hooks.post_hook.as_ref() {
        execute_hook(post_hook, target_label, mode).await?;
    }

    Ok(())
}

async fn execute_plan(plan: &ExecutionPlan, target_label: &str, mode: OutputMode) -> Result<()> {
    let Some(program) = plan.program.as_ref() else {
        return Ok(());
    };

    let mut command = Command::new(program);
    command.args(&plan.args).current_dir(&plan.cwd);

    let status = run_child(&mut command, target_label, "cmd", mode)
        .await
        .with_context(|| {
            format!(
                "failed to execute {} in {} — is it installed and on your PATH?",
                program,
                plan.cwd.display()
            )
        })?;

    if !status.success() {
        return Err(RunError::CommandFailed {
            command: plan.display.clone(),
            exit_code: status.code().unwrap_or(1),
        }
        .into());
    }

    Ok(())
}

async fn execute_hook(hook: &ResolvedHook, target_label: &str, mode: OutputMode) -> Result<()> {
    let s = Styles::default();
    println!(
        "  {} {}",
        s.dim.apply_to("[hook]"),
        s.dim.apply_to(&hook.display),
    );
    println!();

    let mut command = Command::new(&hook.program);
    command.args(&hook.args).current_dir(&hook.cwd);

    let stage = hook.phase.to_string();
    let status = run_child(&mut command, target_label, &stage, mode)
        .await
        .with_context(|| {
            format!(
                "failed to execute hook \"{}\" ({}) — is it installed and on your PATH?",
                hook.name, hook.program
            )
        })?;

    if !status.success() {
        return Err(RunError::HookFailed {
            hook_name: hook.name.clone(),
            exit_code: status.code().unwrap_or(1),
        }
        .into());
    }

    Ok(())
}

/// Spawn a child process and wait for it, delivering output per `mode`.
///
/// `kill_on_drop` ensures that when a fail-fast abort drops the future, the
/// child process is killed instead of lingering as an orphan.
async fn run_child(
    command: &mut Command,
    target_label: &str,
    stage: &str,
    mode: OutputMode,
) -> Result<std::process::ExitStatus> {
    command.kill_on_drop(true);

    if mode == OutputMode::Inherit {
        let mut child = command.spawn()?;
        return Ok(child.wait().await?);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    apply_color_envs(command);
    let mut child = command.spawn()?;

    let prefix = format!("[target:{target_label}/{stage}]");
    let stdout_task = child
        .stdout
        .take()
        .map(|stdout| tokio::spawn(stream_output(stdout, prefix.clone(), false)));
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(stream_output(stderr, prefix.clone(), true)));

    let status = child.wait().await?;

    if let Some(task) = stdout_task {
        task.await
            .context("stdout stream task panicked")?
            .context("failed to stream command stdout")?;
    }
    if let Some(task) = stderr_task {
        task.await
            .context("stderr stream task panicked")?
            .context("failed to stream command stderr")?;
    }

    Ok(status)
}

async fn stream_output<R>(reader: R, prefix: String, is_stderr: bool) -> Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if is_stderr {
            eprintln!("{prefix} {line}");
        } else {
            println!("{prefix} {line}");
        }
    }
    Ok(())
}

/// Encourage color output from piped children (they can't see a TTY).
fn apply_color_envs(command: &mut Command) {
    if std::env::var_os("NO_COLOR").is_none() {
        command
            .env("CLICOLOR_FORCE", "1")
            .env("FORCE_COLOR", "1")
            .env("CARGO_TERM_COLOR", "always")
            .env("COLORTERM", "truecolor");
        if std::env::var_os("TERM").is_none() {
            command.env("TERM", "xterm-256color");
        }
    }
}
