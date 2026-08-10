//! Top-level orchestration for `buona run` — workspace/package resolution,
//! plan resolution, and process execution.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::styles::Styles;
use crate::workspace;

use super::detect_cmd;
use super::error::RunError;
use super::executor;
use super::output;
use super::planner::{TargetRunPlan, resolve_target_run_plan};
use super::targets::{resolve_local_target, resolve_targets};
use super::types::{BuildSystem, FailPolicy};

/// True when `find_workspace_root` failed because no marker was found (vs I/O).
fn is_not_in_workspace_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string().starts_with("not inside a workspace"))
}

/// Resolve an optional workspace root.
///
/// Returns `Ok(Some(root))` when a `buona.workspace.json` ancestor exists,
/// `Ok(None)` when none is found, and propagates real I/O failures.
pub(super) async fn find_optional_workspace_root(cwd: &Path) -> Result<Option<PathBuf>> {
    match workspace::find_workspace_root(cwd).await {
        Ok(root) => Ok(Some(root)),
        Err(e) if is_not_in_workspace_error(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `--target` / `--recursive` only make sense inside a workspace.
pub(super) fn require_workspace_for_multi_target(
    has_workspace: bool,
    recursive: bool,
    targets: &[String],
) -> Result<(), RunError> {
    if !has_workspace && (recursive || !targets.is_empty()) {
        return Err(RunError::ConfigError(
            "--target/-t and --recursive require a buona workspace".to_string(),
        ));
    }
    Ok(())
}

/// CLI options for the run command, parsed by clap and passed from main.
pub(crate) struct RunOptions {
    pub(crate) system: Option<BuildSystem>,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
    pub(crate) targets: Vec<String>,
    pub(crate) recursive: bool,
    pub(crate) parallel: bool,
    pub(crate) jobs: Option<usize>,
    pub(crate) fail_policy: Option<FailPolicy>,
    /// The command to run (e.g. "build", "test", "lint").
    pub(crate) command: String,
    /// Additional arguments passed through to the underlying tool (after `--`).
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EffectiveExecution {
    pub(super) parallel: bool,
    pub(super) jobs: usize,
    pub(super) fail_policy: FailPolicy,
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1)
}

/// Execute the run command.
pub(crate) async fn execute(options: RunOptions) -> Result<()> {
    let s = Styles::default();

    if crate::output::is_json() && !options.dry_run {
        return Err(RunError::ConfigError(
            "JSON output for `buona run` requires --dry-run because child processes own stdout"
                .to_string(),
        )
        .into());
    }

    let command_name = &options.command;
    let extra_args = &options.args;
    let execution = resolve_effective_execution(&options)?;

    if options.recursive && !options.targets.is_empty() {
        return Err(RunError::ConfigError(
            "--recursive cannot be combined with --target/-t".to_string(),
        )
        .into());
    }

    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = find_optional_workspace_root(&cwd).await?;
    require_workspace_for_multi_target(ws_root.is_some(), options.recursive, &options.targets)?;

    if let Some(ref root) = ws_root
        && options.recursive
    {
        return executor::execute_recursive(&options, root, execution).await;
    }

    let targets = match ws_root {
        Some(ref root) => resolve_targets(&cwd, root, &options.targets, false).await?,
        None => vec![resolve_local_target(&cwd).await?],
    };

    let mut target_plans: Vec<TargetRunPlan> = Vec::new();

    for target in targets {
        let target_plan =
            resolve_target_run_plan(target, command_name, extra_args, options.system).await?;

        output::print_plan_info(
            &s,
            &target_plan.target.label(),
            &target_plan.plan,
            options.verbose,
        );

        if options.verbose {
            output::print_hook_info(&s, &target_plan.hooks);
        }

        if options.dry_run {
            output::print_dry_run_stage(
                &s,
                &format!("target:{}/pre", target_plan.target.label()),
                target_plan
                    .hooks
                    .pre_hook
                    .as_ref()
                    .map(|x| x.display.as_str()),
            );
            output::print_dry_run_command_stage(
                &s,
                &format!("target:{}/cmd", target_plan.target.label()),
                &target_plan.plan,
                options.verbose,
            );
            output::print_dry_run_stage(
                &s,
                &format!("target:{}/post", target_plan.target.label()),
                target_plan
                    .hooks
                    .post_hook
                    .as_ref()
                    .map(|x| x.display.as_str()),
            );
            target_plans.push(target_plan);
            continue;
        }

        target_plans.push(target_plan);
    }

    if options.dry_run && crate::output::is_json() {
        let targets = target_plans
            .iter()
            .map(output::target_plan_json)
            .collect::<Vec<_>>();
        crate::output::print_json(&serde_json::json!({
            "command": options.command,
            "args": options.args,
            "dry_run": true,
            "execution": {
                "parallel": execution.parallel,
                "jobs": execution.jobs,
                "fail_policy": execution.fail_policy.to_string(),
            },
            "targets": targets,
        }))?;
    } else if options.dry_run {
        crate::textln!();
    } else {
        executor::execute_target_plans(target_plans, execution).await?;
    }

    Ok(())
}
pub(crate) async fn detect(targets: Vec<String>, recursive: bool) -> Result<()> {
    detect_cmd::detect(targets, recursive).await
}

fn resolve_effective_execution(options: &RunOptions) -> Result<EffectiveExecution> {
    let parallel = options.parallel;
    let jobs = options.jobs.unwrap_or_else(default_jobs);
    if jobs == 0 {
        return Err(RunError::ConfigError("jobs must be at least 1".to_string()).into());
    }

    let fail_policy = options.fail_policy.unwrap_or(FailPolicy::FailFast);

    Ok(EffectiveExecution {
        parallel,
        jobs,
        fail_policy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_target_allowed_inside_workspace() {
        assert!(require_workspace_for_multi_target(true, true, &[]).is_ok());
        assert!(require_workspace_for_multi_target(true, false, &["api".to_string()]).is_ok());
    }

    #[test]
    fn multi_target_rejected_outside_workspace() {
        let err = require_workspace_for_multi_target(false, true, &[]).unwrap_err();
        assert!(matches!(err, RunError::ConfigError(_)));

        let err =
            require_workspace_for_multi_target(false, false, &["root".to_string()]).unwrap_err();
        assert!(matches!(err, RunError::ConfigError(_)));
    }

    #[test]
    fn local_single_target_allowed_outside_workspace() {
        assert!(require_workspace_for_multi_target(false, false, &[]).is_ok());
    }
}
