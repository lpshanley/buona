//! Top-level orchestration for `buona run` — workspace/package resolution,
//! plan resolution, and process execution.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::styles::Styles;
use crate::workspace;

use super::config::load_package_config;
use super::detect::detect_all_systems;
use super::error::RunError;
use super::hooks::{self, HookResolution};
use super::resolve::{ResolveInput, resolve_plan};
use super::types::{ExecutionPlan, HookSource, PlanKind, ResolvedHook};

/// CLI options for the run command, parsed by clap and passed from main.
pub(crate) struct RunOptions {
    pub(crate) system: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
    /// The command to run (e.g. "build", "test", "lint").
    pub(crate) command: String,
    /// Additional arguments passed through to the underlying tool (after `--`).
    pub(crate) args: Vec<String>,
}

/// Execute the run command.
pub(crate) fn execute(options: RunOptions) -> Result<()> {
    let s = Styles::default();

    let command_name = &options.command;
    let extra_args = &options.args;

    // 2. Resolve workspace + package
    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = workspace::find_workspace_root(&cwd).map_err(|_| {
        RunError::NotInWorkspace(
            "not inside a buona workspace (no buona.workspace.json found)\n  \
             Run this command from within a workspace."
                .to_string(),
        )
    })?;

    let pkg_dir = resolve_package_dir(&cwd, &ws_root)?;
    let pkg_name = pkg_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // 3. Load per-package config (buona.json)
    let package_config = load_package_config(&pkg_dir)
        .map_err(|e| RunError::ConfigError(format!("{e}")))?;

    // Extract hook-related fields before moving config into ResolveInput
    let hooks_dir = package_config
        .as_ref()
        .map(|c| c.hooks_dir.clone())
        .unwrap_or_else(|| ".buona/hooks".to_string());
    let explicit_hooks = package_config
        .as_ref()
        .map(|c| c.hooks.clone())
        .unwrap_or_default();

    // 4. Resolve the execution plan
    let input = ResolveInput {
        package_dir: pkg_dir.clone(),
        command: command_name.clone(),
        extra_args: extra_args.to_vec(),
        cli_system: options.system,
        package_config,
    };

    let plan = resolve_plan(&input)?;

    // 5. Resolve hooks (only for standard commands, not proxied ones)
    let hook_resolution = if plan.kind != PlanKind::Proxy {
        let hooks_dir_path = pkg_dir.join(&hooks_dir);
        let convention_hooks = hooks::scan_hooks_dir(&hooks_dir_path);

        let hook_input = hooks::HookResolveInput {
            command: command_name.clone(),
            package_dir: pkg_dir,
            explicit_hooks,
            convention_hooks,
        };
        Some(hooks::resolve_hooks(&hook_input)?)
    } else {
        None
    };

    // Print warnings from hook resolution
    if let Some(ref resolution) = hook_resolution {
        for warning in &resolution.warnings {
            eprintln!(
                "  {} hook \"{}\": {}",
                s.yellow.apply_to("warning:"),
                warning.hook_name,
                warning.message,
            );
        }
    }

    // 6. Print resolution info
    print_plan_info(&s, &pkg_name, &plan, options.verbose);

    if options.verbose {
        if let Some(ref resolution) = hook_resolution {
            print_hook_info(&s, resolution);
        }
    }

    // 7. Execute or dry-run
    if options.dry_run {
        print_dry_run_hooks(&s, &hook_resolution);
        println!(
            "  {} (dry run — not executing)",
            s.dim.apply_to("---")
        );
        println!();
        return Ok(());
    }

    execute_with_hooks(&plan, hook_resolution)
}

/// Print the auto-detected build system and all marker files found.
pub(crate) fn detect() -> Result<()> {
    let s = Styles::default();

    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = workspace::find_workspace_root(&cwd).map_err(|_| {
        RunError::NotInWorkspace(
            "not inside a buona workspace (no buona.workspace.json found)\n  \
             Run this command from within a workspace."
                .to_string(),
        )
    })?;

    let pkg_dir = resolve_package_dir(&cwd, &ws_root)?;
    let pkg_name = pkg_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let detections = detect_all_systems(&pkg_dir);

    println!();
    if detections.is_empty() {
        println!(
            "  {} No build system detected in {}",
            s.dim.apply_to("—"),
            s.cyan.apply_to(&pkg_name),
        );
    } else {
        let winner = &detections[0];
        println!(
            "  {} {} (via {})",
            s.green.apply_to("detected:"),
            s.bold.apply_to(winner.system.to_string()),
            s.dim.apply_to(&winner.marker),
        );

        if detections.len() > 1 {
            println!();
            println!(
                "  {}",
                s.dim.apply_to("Other marker files found:")
            );
            for d in &detections[1..] {
                println!(
                    "  {}  {} (via {})",
                    s.dim.apply_to("·"),
                    d.system,
                    s.dim.apply_to(&d.marker),
                );
            }
        }
    }
    println!();

    Ok(())
}

/// Determine the package root directory from the current working directory.
///
/// Finds which `src/<name>/` directory the cwd is inside. Returns the package
/// directory (e.g., `ws_root/src/my-pkg`).
fn resolve_package_dir(cwd: &Path, ws_root: &Path) -> Result<PathBuf, RunError> {
    let src_dir = ws_root.join("src");

    if let Ok(relative) = cwd.strip_prefix(&src_dir) {
        // The first component of the relative path is the package name
        if let Some(pkg_component) = relative.components().next() {
            let pkg_name = pkg_component.as_os_str().to_string_lossy();
            let pkg_dir = src_dir.join(pkg_name.as_ref());
            if pkg_dir.is_dir() {
                return Ok(pkg_dir);
            }
        }
    }

    Err(RunError::NoPackageResolved(
        "could not determine which package you are in.\n  \
         Run this command from within a package directory (under src/)."
            .to_string(),
    ))
}

fn print_plan_info(s: &Styles, pkg_name: &str, plan: &ExecutionPlan, verbose: bool) {
    println!();
    println!(
        "  {} {} in {}",
        s.dim.apply_to("→"),
        s.bold.apply_to(&plan.display),
        s.cyan.apply_to(pkg_name),
    );

    if verbose {
        println!(
            "  {}  system: {}",
            s.dim.apply_to("│"),
            plan.system
        );
        println!(
            "  {}  kind: {:?}",
            s.dim.apply_to("│"),
            plan.kind
        );
        println!(
            "  {}  cwd: {}",
            s.dim.apply_to("│"),
            plan.cwd.display()
        );
    }
    println!();
}

fn execute_with_hooks(
    plan: &ExecutionPlan,
    hook_resolution: Option<HookResolution>,
) -> Result<()> {
    let (pre_hook, post_hook) = match hook_resolution {
        Some(res) => (res.pre_hook, res.post_hook),
        None => (None, None),
    };

    // 1. Run pre-hook if present
    if let Some(ref hook) = pre_hook {
        let status = execute_hook(hook)?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    // 2. Run main command
    let status = Command::new(&plan.program)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to execute {} — is it installed and on your PATH?",
                plan.program
            )
        })?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    // 3. Run post-hook if present
    if let Some(ref hook) = post_hook {
        let status = execute_hook(hook)?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}

fn execute_hook(hook: &ResolvedHook) -> Result<std::process::ExitStatus> {
    let s = Styles::default();
    println!(
        "  {} {}",
        s.dim.apply_to("[hook]"),
        s.dim.apply_to(&hook.display),
    );
    println!();

    Command::new(&hook.program)
        .args(&hook.args)
        .current_dir(&hook.cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to execute hook \"{}\" ({}) — is it installed and on your PATH?",
                hook.name, hook.program
            )
        })
}

fn print_hook_info(s: &Styles, resolution: &HookResolution) {
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

fn print_dry_run_hooks(s: &Styles, resolution: &Option<HookResolution>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a workspace with a package.
    fn setup_workspace_with_package(ws_dir: &Path, ws_name: &str, pkg_name: &str) -> PathBuf {
        fs::create_dir_all(ws_dir.join("src").join(pkg_name)).unwrap();
        let json = serde_json::json!({ "name": ws_name });
        fs::write(
            ws_dir.join("buona.workspace.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
        ws_dir.join("src").join(pkg_name)
    }

    // ── resolve_package_dir tests ────────────────────────────────

    #[test]
    fn resolve_from_package_root() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let result = resolve_package_dir(&pkg_dir, dir.path()).unwrap();
        assert_eq!(result, pkg_dir);
    }

    #[test]
    fn resolve_from_deep_inside_package() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let deep = pkg_dir.join("src").join("nested");
        fs::create_dir_all(&deep).unwrap();

        let result = resolve_package_dir(&deep, dir.path()).unwrap();
        assert_eq!(result, pkg_dir);
    }

    #[test]
    fn resolve_fails_at_workspace_root() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let result = resolve_package_dir(dir.path(), dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn resolve_fails_at_src_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let src = dir.path().join("src");
        let result = resolve_package_dir(&src, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn resolve_with_multiple_packages() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");
        fs::create_dir_all(dir.path().join("src").join("pkg-b")).unwrap();

        let result_a = resolve_package_dir(
            &dir.path().join("src").join("pkg-a"),
            dir.path(),
        )
        .unwrap();
        let result_b = resolve_package_dir(
            &dir.path().join("src").join("pkg-b"),
            dir.path(),
        )
        .unwrap();

        assert_eq!(
            result_a.file_name().unwrap().to_string_lossy(),
            "pkg-a"
        );
        assert_eq!(
            result_b.file_name().unwrap().to_string_lossy(),
            "pkg-b"
        );
    }
}
