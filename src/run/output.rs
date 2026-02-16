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
        match plan.system {
            Some(system) => println!("  {}  system: {}", s.dim.apply_to("│"), system),
            None => println!("  {}  system: {}", s.dim.apply_to("│"), s.dim.apply_to("none")),
        }
        println!("  {}  kind: {:?}", s.dim.apply_to("│"), plan.kind);
        if let Some(reason) = plan.skip_reason {
            println!(
                "  {}  skipped: {}",
                s.dim.apply_to("│"),
                s.dim.apply_to(reason.label()),
            );
        }
        println!("  {}  cwd: {}", s.dim.apply_to("│"), plan.cwd.display());
    }
    println!();
}

pub(super) fn print_recursive_graph_header(s: &Styles) {
    println!();
    println!("  {}", s.bold.apply_to("dry-run execution graph"));
    println!("  {}", s.dim.apply_to("──────────────────────"));
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

pub(super) fn print_dry_run_stage(s: &Styles, stage_name: &str, command: Option<&str>) {
    match command {
        Some(cmd) => println!(
            "  {} {} {}",
            s.dim.apply_to("·"),
            s.bold.apply_to(stage_name),
            cmd,
        ),
        None => println!(
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

    println!(
        "  {} {} {}{}",
        s.dim.apply_to("·"),
        s.bold.apply_to(stage_name),
        s.dim.apply_to("skipped"),
        s.dim.apply_to(suffix),
    );
}

fn format_display(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}
