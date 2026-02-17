//! Command execution helpers for `buona run`.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::process::Command;

use crate::styles::Styles;

use super::RunOptions;
use super::error::RunError;
use super::hooks::HookResolution;
use super::output;
use super::planner::resolve_target_run_plan;
use super::targets::{ExecutionTarget, list_workspace_package_targets};
use super::types::{ExecutionPlan, ResolvedHook};

pub(super) async fn execute_recursive(options: &RunOptions, ws_root: &Path) -> Result<()> {
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

    if let Some(pre) = ws_plan.hooks.pre_hook.as_ref() {
        execute_hook(pre).await?;
    }

    execute_plan(&ws_plan.plan).await?;

    for pkg in &pkg_plans {
        execute_with_hooks(&pkg.plan, &pkg.hooks).await?;
    }

    if let Some(post) = ws_plan.hooks.post_hook.as_ref() {
        execute_hook(post).await?;
    }

    Ok(())
}

pub(super) async fn execute_with_hooks(plan: &ExecutionPlan, hooks: &HookResolution) -> Result<()> {
    if let Some(pre_hook) = hooks.pre_hook.as_ref() {
        execute_hook(pre_hook).await?;
    }

    execute_plan(plan).await?;

    if let Some(post_hook) = hooks.post_hook.as_ref() {
        execute_hook(post_hook).await?;
    }

    Ok(())
}

async fn execute_plan(plan: &ExecutionPlan) -> Result<()> {
    let Some(program) = plan.program.as_ref() else {
        return Ok(());
    };

    let status = Command::new(program)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .status()
        .await
        .with_context(|| {
            format!(
                "failed to execute {} — is it installed and on your PATH?",
                program
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

async fn execute_hook(hook: &ResolvedHook) -> Result<()> {
    let s = Styles::default();
    println!(
        "  {} {}",
        s.dim.apply_to("[hook]"),
        s.dim.apply_to(&hook.display),
    );
    println!();

    let status = Command::new(&hook.program)
        .args(&hook.args)
        .current_dir(&hook.cwd)
        .status()
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
