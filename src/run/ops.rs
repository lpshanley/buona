//! Top-level orchestration for `buona run` — workspace/package resolution,
//! plan resolution, and process execution.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::styles::Styles;
use crate::workspace;

use super::config::load_package_config;
use super::detect::detect_all_systems;
use super::error::RunError;
use super::hooks::{self, HookResolution};
use super::output;
use super::resolve::{ResolveInput, resolve_plan};
use super::types::{ExecutionPlan, ResolvedHook};

/// CLI options for the run command, parsed by clap and passed from main.
pub(crate) struct RunOptions {
    pub(crate) system: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) verbose: bool,
    pub(crate) targets: Vec<String>,
    pub(crate) recursive: bool,
    /// The command to run (e.g. "build", "test", "lint").
    pub(crate) command: String,
    /// Additional arguments passed through to the underlying tool (after `--`).
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Clone)]
struct ExecutionTarget {
    name: String,
    dir: PathBuf,
    is_workspace_root: bool,
}

impl ExecutionTarget {
    fn label(&self) -> String {
        if self.is_workspace_root {
            "root".to_string()
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug)]
struct TargetRunPlan {
    target: ExecutionTarget,
    plan: ExecutionPlan,
    hooks: Option<HookResolution>,
}

/// Execute the run command.
pub(crate) fn execute(options: RunOptions) -> Result<()> {
    let s = Styles::default();

    let command_name = &options.command;
    let extra_args = &options.args;

    if options.recursive && !options.targets.is_empty() {
        return Err(RunError::ConfigError(
            "--recursive cannot be combined with --target/-t".to_string(),
        )
        .into());
    }

    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = workspace::find_workspace_root(&cwd).map_err(|_| {
        RunError::NotInWorkspace(
            "not inside a buona workspace (no buona.workspace.json found)\n  \
             Run this command from within a workspace."
                .to_string(),
        )
    })?;

    if options.recursive {
        return execute_recursive(&options, &ws_root);
    }

    let targets = resolve_targets(&cwd, &ws_root, &options.targets, false)?;

    for target in targets {
        let target_plan = resolve_target_run_plan(
            target,
            command_name,
            extra_args,
            options.system.clone(),
        )?;

        output::print_plan_info(&s, &target_plan.target.label(), &target_plan.plan, options.verbose);

        if options.verbose
            && let Some(ref resolution) = target_plan.hooks
        {
            output::print_hook_info(&s, resolution);
        }

        if options.dry_run {
            output::print_dry_run_stage(
                &s,
                &format!("target:{}/pre", target_plan.target.label()),
                target_plan
                    .hooks
                    .as_ref()
                    .and_then(|h| h.pre_hook.as_ref().map(|x| x.display.as_str())),
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
                    .as_ref()
                    .and_then(|h| h.post_hook.as_ref().map(|x| x.display.as_str())),
            );
            continue;
        }

        execute_with_hooks(&target_plan.plan, target_plan.hooks)?;
    }

    if options.dry_run {
        println!();
    }

    Ok(())
}

fn execute_recursive(options: &RunOptions, ws_root: &Path) -> Result<()> {
    let s = Styles::default();
    let pkg_targets = list_workspace_package_targets(ws_root)?;
    let ws_target = ExecutionTarget {
        name: "root".to_string(),
        dir: ws_root.to_path_buf(),
        is_workspace_root: true,
    };

    let ws_plan = resolve_target_run_plan(
        ws_target,
        &options.command,
        &options.args,
        options.system.clone(),
    )?;

    let mut pkg_plans = Vec::new();
    for target in pkg_targets {
        pkg_plans.push(resolve_target_run_plan(
            target,
            &options.command,
            &options.args,
            options.system.clone(),
        )?);
    }

    if options.dry_run {
        output::print_recursive_graph_header(&s);
        output::print_dry_run_stage(
            &s,
            "workspace/pre",
            ws_plan
                .hooks
                .as_ref()
                .and_then(|h| h.pre_hook.as_ref().map(|x| x.display.as_str())),
        );
        output::print_dry_run_command_stage(&s, "workspace/cmd", &ws_plan.plan, options.verbose);
        for pkg in &pkg_plans {
            let label = pkg.target.label();
            output::print_dry_run_stage(
                &s,
                &format!("pkg:{label}/pre"),
                pkg.hooks
                    .as_ref()
                    .and_then(|h| h.pre_hook.as_ref().map(|x| x.display.as_str())),
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
                pkg.hooks
                    .as_ref()
                    .and_then(|h| h.post_hook.as_ref().map(|x| x.display.as_str())),
            );
        }
        output::print_dry_run_stage(
            &s,
            "workspace/post",
            ws_plan
                .hooks
                .as_ref()
                .and_then(|h| h.post_hook.as_ref().map(|x| x.display.as_str())),
        );
        println!();
        return Ok(());
    }

    if let Some(ref hooks) = ws_plan.hooks {
        if let Some(ref pre) = hooks.pre_hook {
            let status = execute_hook(pre)?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    execute_plan(&ws_plan.plan)?;

    for pkg in &pkg_plans {
        execute_with_hooks(&pkg.plan, pkg.hooks.clone())?;
    }

    if let Some(ref hooks) = ws_plan.hooks {
        if let Some(ref post) = hooks.post_hook {
            let status = execute_hook(post)?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}

fn resolve_target_run_plan(
    target: ExecutionTarget,
    command_name: &str,
    extra_args: &[String],
    cli_system: Option<String>,
) -> Result<TargetRunPlan> {
    let s = Styles::default();

    let package_config =
        load_package_config(&target.dir).map_err(|e| RunError::ConfigError(format!("{e}")))?;

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
    let plan = resolve_plan(&input)?;

    let hooks_dir_path = target.dir.join(&hooks_dir);
    let convention_hooks = hooks::scan_hooks_dir(&hooks_dir_path);
    let hook_input = hooks::HookResolveInput {
        command: command_name.to_string(),
        package_dir: target.dir.clone(),
        explicit_hooks,
        convention_hooks,
    };
    let hook_resolution = Some(hooks::resolve_hooks(&hook_input)?);

    if let Some(ref resolution) = hook_resolution {
        output::print_hook_warnings(&s, resolution);
    }

    Ok(TargetRunPlan {
        target,
        plan,
        hooks: hook_resolution,
    })
}

fn resolve_targets(
    cwd: &Path,
    ws_root: &Path,
    target_names: &[String],
    recursive: bool,
) -> Result<Vec<ExecutionTarget>, RunError> {
    if recursive {
        let mut targets = vec![ExecutionTarget {
            name: "root".to_string(),
            dir: ws_root.to_path_buf(),
            is_workspace_root: true,
        }];
        targets.extend(list_workspace_package_targets(ws_root)?);
        return Ok(targets);
    }

    if target_names.is_empty() {
        return Ok(vec![resolve_closest_target(cwd, ws_root)?]);
    }

    let mut targets = Vec::new();
    for target_name in target_names {
        if target_name == "root" {
            targets.push(ExecutionTarget {
                name: "root".to_string(),
                dir: ws_root.to_path_buf(),
                is_workspace_root: true,
            });
            continue;
        }
        let pkg_dir = ws_root.join("src").join(target_name);
        if !pkg_dir.is_dir() {
            return Err(RunError::ConfigError(format!(
                "unknown target \"{target_name}\" in workspace {}",
                ws_root.display()
            )));
        }
        targets.push(ExecutionTarget {
            name: target_name.clone(),
            dir: pkg_dir,
            is_workspace_root: false,
        });
    }

    Ok(targets)
}

fn resolve_closest_target(cwd: &Path, ws_root: &Path) -> Result<ExecutionTarget, RunError> {
    if let Ok(pkg_dir) = resolve_package_dir(cwd, ws_root) {
        let pkg_name = pkg_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        return Ok(ExecutionTarget {
            name: pkg_name,
            dir: pkg_dir,
            is_workspace_root: false,
        });
    }

    Ok(ExecutionTarget {
        name: "root".to_string(),
        dir: ws_root.to_path_buf(),
        is_workspace_root: true,
    })
}

fn list_workspace_package_targets(ws_root: &Path) -> Result<Vec<ExecutionTarget>, RunError> {
    let src_dir = ws_root.join("src");
    let mut targets = Vec::new();
    if !src_dir.is_dir() {
        return Ok(targets);
    }

    let entries = fs::read_dir(&src_dir).map_err(|e| {
        RunError::ConfigError(format!("could not read src directory {}: {e}", src_dir.display()))
    })?;
    for entry in entries {
        let entry = entry
            .map_err(|e| RunError::ConfigError(format!("could not read src entry: {e}")))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        targets.push(ExecutionTarget {
            name,
            dir: path,
            is_workspace_root: false,
        });
    }
    targets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(targets)
}

/// Print the auto-detected build system and all marker files found.
pub(crate) fn detect(targets: Vec<String>, recursive: bool) -> Result<()> {
    let s = Styles::default();

    if recursive && !targets.is_empty() {
        return Err(RunError::ConfigError(
            "--recursive cannot be combined with --target/-t".to_string(),
        )
        .into());
    }

    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = workspace::find_workspace_root(&cwd).map_err(|_| {
        RunError::NotInWorkspace(
            "not inside a buona workspace (no buona.workspace.json found)\n  \
             Run this command from within a workspace."
                .to_string(),
        )
    })?;

    let detect_targets = resolve_targets(&cwd, &ws_root, &targets, recursive)?;

    println!();
    for target in detect_targets {
        let detections = detect_all_systems(&target.dir);
        println!("  {} {}", s.bold.apply_to("target:"), s.cyan.apply_to(target.label()));
        if detections.is_empty() {
            println!("    {} noop", s.dim.apply_to("—"));
            continue;
        }

        let winner = &detections[0];
        println!(
            "    {} {} (via {})",
            s.green.apply_to("detected:"),
            s.bold.apply_to(winner.system.to_string()),
            s.dim.apply_to(&winner.marker),
        );

        if detections.len() > 1 {
            println!("    {}", s.dim.apply_to("Other marker files found:"));
            for d in &detections[1..] {
                println!(
                    "    {}  {} (via {})",
                    s.dim.apply_to("·"),
                    d.system,
                    s.dim.apply_to(&d.marker),
                );
            }
        }
        println!();
    }

    Ok(())
}

fn execute_plan(plan: &ExecutionPlan) -> Result<()> {
    let Some(program) = plan.program.as_ref() else {
        return Ok(());
    };

    let status = Command::new(program)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .status()
        .with_context(|| {
            format!(
                "failed to execute {} — is it installed and on your PATH?",
                program
            )
        })?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

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

fn execute_with_hooks(plan: &ExecutionPlan, hook_resolution: Option<HookResolution>) -> Result<()> {
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

    // 2. Run main command (if runnable)
    execute_plan(plan)?;

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

        let result_a =
            resolve_package_dir(&dir.path().join("src").join("pkg-a"), dir.path()).unwrap();
        let result_b =
            resolve_package_dir(&dir.path().join("src").join("pkg-b"), dir.path()).unwrap();

        assert_eq!(result_a.file_name().unwrap().to_string_lossy(), "pkg-a");
        assert_eq!(result_b.file_name().unwrap().to_string_lossy(), "pkg-b");
    }

    #[test]
    fn resolve_targets_closest_defaults_to_package() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");

        let targets = resolve_targets(&pkg_dir, dir.path(), &[], false).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].label(), "pkg-a");
    }

    #[test]
    fn resolve_targets_closest_defaults_to_root() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");

        let targets = resolve_targets(dir.path(), dir.path(), &[], false).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].label(), "root");
    }

    #[test]
    fn resolve_targets_respects_ordered_explicit_targets() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");
        fs::create_dir_all(dir.path().join("src").join("pkg-b")).unwrap();

        let requested = vec!["pkg-b".to_string(), "root".to_string(), "pkg-a".to_string()];
        let targets = resolve_targets(dir.path(), dir.path(), &requested, false).unwrap();
        let labels: Vec<String> = targets.iter().map(|t| t.label()).collect();
        assert_eq!(labels, vec!["pkg-b", "root", "pkg-a"]);
    }

    #[test]
    fn resolve_targets_recursive_includes_root_and_sorted_packages() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "zeta");
        fs::create_dir_all(dir.path().join("src").join("alpha")).unwrap();

        let targets = resolve_targets(dir.path(), dir.path(), &[], true).unwrap();
        let labels: Vec<String> = targets.iter().map(|t| t.label()).collect();
        assert_eq!(labels, vec!["root", "alpha", "zeta"]);
    }
}
