//! Terminal rendering helpers for `buona run`.

use crate::styles::Styles;

use super::hooks::HookResolution;
use super::types::{ExecutionPlan, HookSource};

pub(super) fn print_plan_info(s: &Styles, pkg_name: &str, plan: &ExecutionPlan, verbose: bool) {
    println!();
    println!(
        "  {} {} in {}",
        s.dim.apply_to("→"),
        s.bold.apply_to(&plan.display),
        s.cyan.apply_to(pkg_name),
    );

    if verbose {
        println!("  {}  system: {}", s.dim.apply_to("│"), plan.system);
        println!("  {}  kind: {:?}", s.dim.apply_to("│"), plan.kind);
        println!("  {}  cwd: {}", s.dim.apply_to("│"), plan.cwd.display());
    }
    println!();
}

pub(super) fn print_hook_warnings(s: &Styles, resolution: &HookResolution) {
    for warning in &resolution.warnings {
        eprintln!(
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
    println!("  {}  hooks:", s.dim.apply_to("│"));
    if let Some(ref hook) = resolution.pre_hook {
        let source_label = match hook.source {
            HookSource::Explicit => "explicit",
            HookSource::Convention => "convention",
        };
        println!(
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
        println!(
            "  {}    {}: {} ({})",
            s.dim.apply_to("│"),
            hook.name,
            hook.display,
            s.dim.apply_to(source_label),
        );
    }
    println!();
}

pub(super) fn print_dry_run_hooks(s: &Styles, resolution: &Option<HookResolution>) {
    if let Some(res) = resolution {
        if let Some(hook) = &res.pre_hook {
            println!(
                "  {} {}: {}",
                s.dim.apply_to("[hook]"),
                s.bold.apply_to(&hook.name),
                hook.display,
            );
        }
        if let Some(hook) = &res.post_hook {
            println!(
                "  {} {}: {}",
                s.dim.apply_to("[hook]"),
                s.bold.apply_to(&hook.name),
                hook.display,
            );
        }
    }
}
