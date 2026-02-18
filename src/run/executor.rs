//! Command execution helpers for `buona run`.

use std::path::Path;
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

#[derive(Clone)]
struct OutputSink;

impl OutputSink {
    fn plain() -> Self {
        Self
    }

    fn task_queued(&self, _task: &str) {}

    fn task_started(&self, _task: &str) {}

    fn task_finished(&self, _task: &str, _success: bool) {}

    fn line(&self, _task: &str, line: String, is_stderr: bool) {
        if is_stderr {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
}

pub(super) async fn execute_recursive(
    options: &RunOptions,
    ws_root: &Path,
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

    let sink = OutputSink::plain();

    let result = async {
        let root_task = "root";
        sink.task_queued(root_task);
        sink.task_started(root_task);

        if let Some(pre) = ws_plan.hooks.pre_hook.as_ref() {
            execute_hook(pre, root_task, &sink).await?;
        }
        execute_plan(&ws_plan.plan, root_task, &sink).await?;

        execute_package_stage(pkg_plans, exec, sink.clone()).await?;

        if let Some(post) = ws_plan.hooks.post_hook.as_ref() {
            execute_hook(post, root_task, &sink).await?;
        }
        sink.task_finished(root_task, true);

        Ok(())
    }
    .await;

    if result.is_err() {
        sink.task_finished("root", false);
    }

    result
}

pub(super) async fn execute_target_plans(
    target_plans: Vec<TargetRunPlan>,
    exec: EffectiveExecution,
) -> Result<()> {
    execute_package_stage(target_plans, exec, OutputSink::plain()).await
}

async fn execute_package_stage(
    pkg_plans: Vec<TargetRunPlan>,
    exec: EffectiveExecution,
    sink: OutputSink,
) -> Result<()> {
    if !exec.parallel || exec.jobs <= 1 || pkg_plans.len() <= 1 {
        for pkg in pkg_plans {
            let label = pkg.target.label();
            sink.task_queued(&label);
            sink.task_started(&label);
            let result = execute_with_hooks_inner(&pkg.plan, &pkg.hooks, &label, &sink).await;
            sink.task_finished(&label, result.is_ok());
            result?;
        }
        return Ok(());
    }

    let semaphore = Arc::new(Semaphore::new(exec.jobs));
    let mut set = JoinSet::new();

    for pkg in pkg_plans {
        let label = pkg.target.label();
        sink.task_queued(&label);

        let sem = semaphore.clone();
        let sink_clone = sink.clone();
        set.spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| RunError::ConfigError("parallel scheduler closed".to_string()))?;
            sink_clone.task_started(&label);

            let result = execute_with_hooks_inner(&pkg.plan, &pkg.hooks, &label, &sink_clone).await;
            sink_clone.task_finished(&label, result.is_ok());
            Ok::<(String, Result<()>), RunError>((label, result))
        });
    }

    let mut first_error: Option<anyhow::Error> = None;

    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok((_label, run_result))) => {
                if let Err(err) = run_result {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                    if exec.fail_policy == FailPolicy::FailFast {
                        set.abort_all();
                        break;
                    }
                }
            }
            Ok(Err(err)) => {
                if first_error.is_none() {
                    first_error = Some(err.into());
                }
                if exec.fail_policy == FailPolicy::FailFast {
                    set.abort_all();
                    break;
                }
            }
            Err(join_err) => {
                if first_error.is_none() {
                    first_error = Some(anyhow::anyhow!("parallel task join error: {join_err}"));
                }
                if exec.fail_policy == FailPolicy::FailFast {
                    set.abort_all();
                    break;
                }
            }
        }
    }

    while set.join_next().await.is_some() {}

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(())
}

async fn execute_with_hooks_inner(
    plan: &ExecutionPlan,
    hooks: &HookResolution,
    target_label: &str,
    sink: &OutputSink,
) -> Result<()> {
    if let Some(pre_hook) = hooks.pre_hook.as_ref() {
        execute_hook(pre_hook, target_label, sink).await?;
    }

    execute_plan(plan, target_label, sink).await?;

    if let Some(post_hook) = hooks.post_hook.as_ref() {
        execute_hook(post_hook, target_label, sink).await?;
    }

    Ok(())
}

async fn execute_plan(plan: &ExecutionPlan, target_label: &str, sink: &OutputSink) -> Result<()> {
    let Some(program) = plan.program.as_ref() else {
        return Ok(());
    };

    let mut command = Command::new(program);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_color_envs(&mut command);
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to execute {} — is it installed and on your PATH?",
            program
        )
    })?;

    let prefix = stage_prefix(target_label, "cmd");
    let task_key = target_label.to_string();
    let sink_stdout = sink.clone();
    let sink_stderr = sink.clone();
    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(stream_output(
            stdout,
            prefix.clone(),
            task_key.clone(),
            sink_stdout,
            false,
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(stream_output(
            stderr,
            prefix.clone(),
            task_key.clone(),
            sink_stderr,
            true,
        ))
    });

    let status = child.wait().await.with_context(|| {
        format!(
            "failed while waiting for {} to exit in {}",
            program,
            plan.cwd.display()
        )
    })?;

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

    if !status.success() {
        return Err(RunError::CommandFailed {
            command: plan.display.clone(),
            exit_code: status.code().unwrap_or(1),
        }
        .into());
    }

    Ok(())
}

async fn execute_hook(hook: &ResolvedHook, target_label: &str, sink: &OutputSink) -> Result<()> {
    let s = Styles::default();
    println!(
        "  {} {}",
        s.dim.apply_to("[hook]"),
        s.dim.apply_to(&hook.display),
    );
    println!();

    let mut command = Command::new(&hook.program);
    command
        .args(&hook.args)
        .current_dir(&hook.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_color_envs(&mut command);
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to execute hook \"{}\" ({}) — is it installed and on your PATH?",
            hook.name, hook.program
        )
    })?;

    let stage = hook.phase.to_string();
    let prefix = stage_prefix(target_label, &stage);
    let task_key = target_label.to_string();
    let sink_stdout = sink.clone();
    let sink_stderr = sink.clone();
    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(stream_output(
            stdout,
            prefix.clone(),
            task_key.clone(),
            sink_stdout,
            false,
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(stream_output(
            stderr,
            prefix.clone(),
            task_key.clone(),
            sink_stderr,
            true,
        ))
    });

    let status = child.wait().await.with_context(|| {
        format!(
            "failed while waiting for hook \"{}\" ({}) to exit",
            hook.name, hook.program
        )
    })?;

    if let Some(task) = stdout_task {
        task.await
            .context("hook stdout stream task panicked")?
            .context("failed to stream hook stdout")?;
    }
    if let Some(task) = stderr_task {
        task.await
            .context("hook stderr stream task panicked")?
            .context("failed to stream hook stderr")?;
    }

    if !status.success() {
        return Err(RunError::HookFailed {
            hook_name: hook.name.clone(),
            exit_code: status.code().unwrap_or(1),
        }
        .into());
    }

    Ok(())
}

fn stage_prefix(target_label: &str, stage: &str) -> String {
    format!("[target:{target_label}/{stage}]")
}

async fn stream_output<R>(
    reader: R,
    prefix: String,
    task: String,
    sink: OutputSink,
    is_stderr: bool,
) -> Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        sink.line(&task, format!("{prefix} {line}"), is_stderr);
    }
    Ok(())
}

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
