//! Terminal rendering helpers for `buona run`.

use serde_json::{Value, json};

use crate::styles::Styles;

use super::format::format_display;
use super::hooks::HookResolution;
use super::planner::TargetRunPlan;
use super::types::{ExecutionPlan, HookSource, PlanKind};

pub(super) fn target_plan_json(target_plan: &TargetRunPlan) -> Value {
    let plan = &target_plan.plan;
    json!({
        "target": {
            "name": target_plan.target.label(),
            "kind": if target_plan.target.is_workspace_root { "workspace-root" } else { "package" },
            "directory": target_plan.target.dir,
        },
        "plan": {
            "system": plan.system.map(|system| system.to_string()),
            "kind": plan_kind_label(plan.kind),
            "program": plan.program,
            "args": plan.args,
            "display": plan.display,
            "cwd": plan.cwd,
            "skip_reason": plan.skip_reason.map(|reason| reason.label()),
        },
        "hooks": {
            "pre": target_plan.hooks.pre_hook.as_ref().map(hook_json),
            "post": target_plan.hooks.post_hook.as_ref().map(hook_json),
            "warnings": target_plan.hooks.warnings.iter().map(|warning| json!({
                "hook": warning.hook_name,
                "message": warning.message,
            })).collect::<Vec<_>>(),
        }
    })
}

fn hook_json(hook: &super::types::ResolvedHook) -> Value {
    json!({
        "name": hook.name,
        "phase": hook.phase.to_string(),
        "source": match hook.source {
            HookSource::Explicit => "explicit",
            HookSource::Convention => "convention",
        },
        "program": hook.program,
        "args": hook.args,
        "cwd": hook.cwd,
        "display": hook.display,
    })
}

fn plan_kind_label(kind: PlanKind) -> &'static str {
    match kind {
        PlanKind::Standard => "standard",
        PlanKind::Proxy => "proxy",
        PlanKind::ExecOverride => "exec-override",
        PlanKind::Noop => "noop",
    }
}

pub(super) fn print_plan_info(s: &Styles, pkg_name: &str, plan: &ExecutionPlan, verbose: bool) {
    crate::textln!();
    crate::textln!(
        "  {} {} in {}",
        s.dim.apply_to("→"),
        s.bold.apply_to(&plan.display),
        s.cyan.apply_to(pkg_name),
    );

    if verbose {
        match plan.system {
            Some(system) => crate::textln!("  {}  system: {}", s.dim.apply_to("│"), system),
            None => crate::textln!(
                "  {}  system: {}",
                s.dim.apply_to("│"),
                s.dim.apply_to("none")
            ),
        }
        crate::textln!("  {}  kind: {:?}", s.dim.apply_to("│"), plan.kind);
        if let Some(reason) = plan.skip_reason {
            crate::textln!(
                "  {}  skipped: {}",
                s.dim.apply_to("│"),
                s.dim.apply_to(reason.label()),
            );
        }
        crate::textln!("  {}  cwd: {}", s.dim.apply_to("│"), plan.cwd.display());
    }
    crate::textln!();
}

pub(super) fn print_recursive_graph_header(s: &Styles) {
    crate::textln!();
    crate::textln!("  {}", s.bold.apply_to("dry-run execution graph"));
    crate::textln!("  {}", s.dim.apply_to("──────────────────────"));
}

pub(super) fn print_hook_warnings(s: &Styles, resolution: &HookResolution) {
    for warning in &resolution.warnings {
        crate::text_errln!(
            "  {} hook \"{}\": {}",
            s.yellow.apply_to("warning:"),
            warning.hook_name,
            warning.message,
        );
    }
}

pub(super) fn print_hook_info(s: &Styles, resolution: &HookResolution) {
    let has_hooks = resolution.pre_hook.is_some() || resolution.post_hook.is_some();
    if !has_hooks {
        return;
    }
    crate::textln!("  {}  hooks:", s.dim.apply_to("│"));
    if let Some(ref hook) = resolution.pre_hook {
        let source_label = match hook.source {
            HookSource::Explicit => "explicit",
            HookSource::Convention => "convention",
        };
        crate::textln!(
            "  {}    {}: {} ({})",
            s.dim.apply_to("│"),
            hook.name,
            hook.display,
            s.dim.apply_to(source_label),
        );
    }
    if let Some(ref hook) = resolution.post_hook {
        let source_label = match hook.source {
            HookSource::Explicit => "explicit",
            HookSource::Convention => "convention",
        };
        crate::textln!(
            "  {}    {}: {} ({})",
            s.dim.apply_to("│"),
            hook.name,
            hook.display,
            s.dim.apply_to(source_label),
        );
    }
    crate::textln!();
}

pub(super) fn print_dry_run_stage(s: &Styles, stage_name: &str, command: Option<&str>) {
    match command {
        Some(cmd) => crate::textln!(
            "  {} {} {}",
            s.dim.apply_to("·"),
            s.bold.apply_to(stage_name),
            cmd,
        ),
        None => crate::textln!(
            "  {} {} {}",
            s.dim.apply_to("·"),
            s.bold.apply_to(stage_name),
            s.dim.apply_to("noop"),
        ),
    }
}

pub(super) fn print_dry_run_command_stage(
    s: &Styles,
    stage_name: &str,
    plan: &ExecutionPlan,
    verbose: bool,
) {
    if let Some(cmd) = plan.program.as_ref() {
        return print_dry_run_stage(s, stage_name, Some(&format_display(cmd, &plan.args)));
    }

    let suffix = if verbose {
        plan.skip_reason
            .map(|reason| format!(" [{}]", reason.label()))
            .unwrap_or_default()
    } else {
        String::new()
    };

    crate::textln!(
        "  {} {} {}{}",
        s.dim.apply_to("·"),
        s.bold.apply_to(stage_name),
        s.dim.apply_to("skipped"),
        s.dim.apply_to(suffix),
    );
}
