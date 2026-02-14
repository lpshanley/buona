//! Top-level orchestration for `buona run` — workspace/package resolution,
//! plan resolution, and process execution.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::styles::Styles;
use crate::workspace;

use super::config::load_package_config;
use super::error::RunError;
use super::resolve::{ResolveInput, resolve_plan};
use super::types::ExecutionPlan;

/// CLI options for the run command, parsed by clap and passed from main.
pub(crate) struct RunOptions {
    pub(crate) system: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
    /// Everything after `--`: [command, args...]
    pub(crate) command: Vec<String>,
}

/// Execute the run command.
pub(crate) fn execute(options: RunOptions) -> Result<()> {
    let s = Styles::default();

    // 1. Validate: must have at least one command token
    if options.command.is_empty() {
        anyhow::bail!(
            "no command specified.\n  Usage: buona run [options] -- <command> [args...]"
        );
    }

    let command_name = &options.command[0];
    let extra_args = &options.command[1..];

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

    // 4. Resolve the execution plan
    let input = ResolveInput {
        package_dir: pkg_dir,
        command: command_name.clone(),
        extra_args: extra_args.to_vec(),
        cli_system: options.system,
        package_config,
    };

    let plan = resolve_plan(&input)?;

    // 5. Print resolution info
    print_plan_info(&s, &pkg_name, &plan, options.verbose);

    // 6. Execute or dry-run
    if options.dry_run {
        println!(
            "  {} (dry run — not executing)",
            s.dim.apply_to("---")
        );
        println!();
        return Ok(());
    }

    execute_plan(&plan)
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

fn execute_plan(plan: &ExecutionPlan) -> Result<()> {
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

    Ok(())
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
