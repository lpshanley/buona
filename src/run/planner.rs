//! Plan and hook resolution for an execution target.

use anyhow::Result;

use crate::styles::Styles;

use super::config::load_package_config;
use super::hooks::{self, HookResolution};
use super::output;
use super::resolve::{ResolveInput, resolve_plan};
use super::targets::ExecutionTarget;
use super::types::{BuildSystem, ExecutionPlan};

#[derive(Debug)]
pub(super) struct TargetRunPlan {
    pub(super) target: ExecutionTarget,
    pub(super) plan: ExecutionPlan,
    pub(super) hooks: HookResolution,
}

pub(super) async fn resolve_target_run_plan(
    target: ExecutionTarget,
    command_name: &str,
    extra_args: &[String],
    cli_system: Option<BuildSystem>,
) -> Result<TargetRunPlan> {
    let s = Styles::default();

    let package_config = load_package_config(&target.dir)
        .await
        .map_err(|e| super::error::RunError::ConfigError(format!("{e}")))?;

    let hooks_dir = package_config
        .as_ref()
        .map(|c| c.hooks_dir.clone())
        .unwrap_or_else(|| ".buona/hooks".to_string());

    let explicit_hooks = package_config
        .as_ref()
        .map(|c| c.hooks.clone())
        .unwrap_or_default();

    let input = ResolveInput {
        package_dir: target.dir.clone(),
        command: command_name.to_string(),
        extra_args: extra_args.to_vec(),
        cli_system,
        package_config,
    };
    let plan = resolve_plan(&input).await?;

    let hooks_dir_path = target.dir.join(&hooks_dir);
    let convention_hooks = hooks::scan_hooks_dir(&hooks_dir_path).await;
    let hook_input = hooks::HookResolveInput {
        command: command_name.to_string(),
        package_dir: target.dir.clone(),
        explicit_hooks,
        convention_hooks,
    };
    let hook_resolution = hooks::resolve_hooks(&hook_input)?;

    output::print_hook_warnings(&s, &hook_resolution);

    Ok(TargetRunPlan {
        target,
        plan,
        hooks: hook_resolution,
    })
}
