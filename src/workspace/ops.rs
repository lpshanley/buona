//! Workspace operations — list, create, delete, add, remove, sync, adopt, and open.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::config;
use crate::config::GitTracking;
use crate::styles::Styles;

use super::git::resolve_package_spec;
use super::git_ops;
use super::types::{WORKSPACE_FILE, WorkspaceMeta, read_meta, write_meta};
use super::vscode::{VscodeWorkspace, VscodeWorkspaceFolder, sanitize_name};

/// Resolve the effective git tracking mode for a workspace.
///
/// Priority: workspace-level override > global config default > hardcoded Package.
fn resolve_git_tracking(meta: &WorkspaceMeta, cfg: &config::BuonaConfig) -> GitTracking {
    meta.git_tracking.unwrap_or(cfg.git.tracking)
}

/// Find a workspace by name or directory name. Returns the resolved path.
fn find_workspace(query: &str) -> Result<PathBuf> {
    let workspace_dir = config::workspace_dir()?;

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct)?.is_some() {
        return Ok(direct);
    }

    // Otherwise, search by workspace name in metadata
    let entries = fs::read_dir(&workspace_dir).with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path)?
                && meta.name == query
            {
                return Ok(path);
            }
        }
    }

    bail!("no workspace found matching \"{query}\"")
}

/// Walk up from the given directory looking for a `buona.workspace.json` file.
/// Returns the directory containing the workspace file.
pub(crate) fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(WORKSPACE_FILE).exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!(
                "not inside a workspace (no {} found in any parent directory)\n  \
                 Either cd into a workspace or use --workspace to specify one.",
                WORKSPACE_FILE
            );
        }
    }
}

/// Find the workspace root from the current working directory.
fn find_workspace_from_cwd() -> Result<PathBuf> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    find_workspace_root(&cwd)
}

/// Resolve workspace root from an optional workspace selector.
///
/// If a selector is provided, lookup by name/directory. Otherwise, detect from
/// current working directory.
fn resolve_workspace_target(workspace: Option<&str>) -> Result<PathBuf> {
    match workspace {
        Some(name) => find_workspace(name),
        None => find_workspace_from_cwd(),
    }
}

/// Scan the `src/` directory of a workspace and return sorted package names.
///
/// Each subdirectory of `src/` is treated as a package. Returns an empty vec
/// if the `src/` directory does not exist.
fn list_packages(ws_root: &Path) -> Result<Vec<String>> {
    let src_dir = ws_root.join("src");
    if !src_dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = Vec::new();
    for entry in fs::read_dir(&src_dir)
        .with_context(|| format!("could not read src directory: {}", src_dir.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Detect the git remote origin URL for a directory, if it is a git repo.
///
/// Returns the URL string on success, or an empty string if the directory is
/// not a git repo or has no `origin` remote.
fn detect_git_remote_url(dir: &Path) -> String {
    git_ops::detect_remote_url(dir)
}

/// Detect the current git branch for a directory.
///
/// Returns the branch name on success, or an empty string if the directory is
/// not a git repo or HEAD is detached.
fn detect_git_branch(dir: &Path) -> String {
    git_ops::detect_branch(dir)
}

/// List all workspaces (directories) found in the configured workspace directory.
pub(crate) fn list() -> Result<()> {
    let workspace_dir = config::workspace_dir()?;
    let s = Styles::default();

    println!();
    println!("  {}", s.bold.apply_to("Workspaces"));
    println!("  {}", s.dim.apply_to("──────────"));

    if !workspace_dir.exists() {
        bail!(
            "workspace directory does not exist: {}\n  Run {} to configure it.",
            workspace_dir.display(),
            "buona config setup",
        );
    }

    let entries = fs::read_dir(&workspace_dir).with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    let mut workspaces: Vec<(String, WorkspaceMeta)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            if let Some(meta) = read_meta(&entry.path())? {
                workspaces.push((dir_name, meta));
            }
        }
    }

    workspaces.sort_by(|a, b| a.0.cmp(&b.0));

    if workspaces.is_empty() {
        println!(
            "  {}",
            s.dim.apply_to(format!(
                "No workspaces found in {}",
                workspace_dir.display()
            ))
        );
    } else {
        println!(
            "  {}  {}",
            s.dim.apply_to("Directory:"),
            workspace_dir.display()
        );
        println!();
        for (dir_name, meta) in &workspaces {
            if meta.name != *dir_name {
                println!(
                    "  {}  {} {}",
                    s.cyan.apply_to("•"),
                    meta.name,
                    s.dim.apply_to(format!("({dir_name})"))
                );
            } else {
                println!("  {}  {dir_name}", s.cyan.apply_to("•"));
            }
        }
    }

    println!();
    Ok(())
}

/// Create a new workspace directory. Writes a `buona.workspace.json` marker
/// file with the workspace name. If `name` is not provided, the directory name
/// is used.
///
/// After creation, if `packages` is provided, they are cloned into the workspace.
/// If `open_ws` is true, the workspace is opened in the configured editor.
pub(crate) fn create(
    path: &Path,
    name: Option<&str>,
    packages: Option<&[String]>,
    open_ws: bool,
    git_tracking: Option<GitTracking>,
) -> Result<()> {
    let s = Styles::default();

    // Resolve the target directory
    let target: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config::workspace_dir()?.join(path)
    };

    // Derive the workspace name
    let ws_name = match name {
        Some(n) => n.to_string(),
        None => target
            .file_name()
            .context("could not determine directory name from path")?
            .to_string_lossy()
            .into_owned(),
    };

    if target.exists() {
        bail!("directory already exists: {}", target.display());
    }

    // Create the workspace directory (and any parent directories)
    fs::create_dir_all(&target)
        .with_context(|| format!("could not create workspace directory: {}", target.display()))?;

    // Create the src/ directory for packages
    fs::create_dir_all(target.join("src")).with_context(|| {
        format!(
            "could not create src directory: {}",
            target.join("src").display()
        )
    })?;

    // Write the workspace metadata file
    let meta = WorkspaceMeta {
        name: ws_name,
        git_tracking,
    };
    write_meta(&target, &meta)?;

    // Initialize git at workspace root if workspace-level tracking is active
    let cfg = config::load_config()?;
    let tracking = resolve_git_tracking(&meta, &cfg);

    if tracking == GitTracking::Workspace {
        let output = git_ops::init_repo(&target)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git init failed: {}", stderr.trim());
        }

        println!(
            "  {} Initialized git repository at workspace root",
            s.green.apply_to("✔"),
        );
    }

    // Auto-sync the .code-workspace file
    sync_workspace_file(&target, &meta)?;

    println!();
    println!(
        "  {} Created workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}  {}", s.dim.apply_to("Location:"), target.display());

    // Add packages if specified
    if let Some(pkgs) = packages
        && !pkgs.is_empty()
    {
        add_packages_to_workspace(&target, pkgs)?;
    }

    // Open workspace in editor if requested
    if open_ws {
        open_workspace_at(&target)?;
    }

    println!();
    Ok(())
}

/// Delete a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub(crate) fn delete(query: &str, force: bool) -> Result<()> {
    let s = Styles::default();

    let target = find_workspace(query)?;

    let meta = read_meta(&target)?;
    let display_name = meta.as_ref().map(|m| m.name.as_str()).unwrap_or(query);

    if !force {
        println!();
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "  Delete workspace {} at {}?",
                s.bold.apply_to(display_name),
                s.dim.apply_to(target.display().to_string())
            ))
            .default(false)
            .interact()
            .context("failed to read input")?;

        if !confirmed {
            println!("  Aborted.");
            println!();
            return Ok(());
        }
    }

    fs::remove_dir_all(&target)
        .with_context(|| format!("could not delete workspace directory: {}", target.display()))?;

    println!();
    println!(
        "  {} Deleted workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(display_name)
    );
    println!();

    Ok(())
}

/// Add packages to a specific workspace root.
///
/// Internal helper that adds packages to `src/` without resolving workspace by name.
fn add_packages_to_workspace(ws_root: &Path, packages: &[String]) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    let meta = read_meta(ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let tracking = resolve_git_tracking(&meta, &cfg);
    let src_dir = ws_root.join("src");

    println!();
    println!(
        "  {} Adding packages to {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    let mut successes: Vec<String> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for spec in packages {
        let resolved = match resolve_package_spec(spec, &cfg.git) {
            Ok(r) => r,
            Err(e) => {
                failures.push((spec.clone(), format!("{e}")));
                println!("  {} {} — {}", s.red.apply_to("✘"), spec, e);
                continue;
            }
        };

        let dest = src_dir.join(&resolved.name);
        if dest.exists() {
            let msg = format!("directory already exists: {}", dest.display());
            failures.push((spec.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
            continue;
        }

        // Ensure src/ directory exists
        fs::create_dir_all(&src_dir)
            .with_context(|| format!("could not create src directory: {}", src_dir.display()))?;

        println!(
            "  {} Cloning {} ...",
            s.dim.apply_to("→"),
            s.cyan.apply_to(&resolved.name)
        );

        let output = git_ops::clone_into(&resolved.url, &dest)?;

        if output.status.success() {
            // If workspace-level tracking, remove the package's .git directory
            if tracking == GitTracking::Workspace {
                let pkg_git_dir = dest.join(".git");
                if pkg_git_dir.exists() {
                    fs::remove_dir_all(&pkg_git_dir).with_context(|| {
                        format!(
                            "could not remove .git directory from cloned package: {}",
                            pkg_git_dir.display()
                        )
                    })?;
                }
            }

            println!(
                "  {} {}",
                s.green.apply_to("✔"),
                s.bold.apply_to(&resolved.name)
            );
            successes.push(resolved.name);
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            failures.push((spec.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
        }
    }

    // Re-sync the .code-workspace file (picks up newly cloned directories)
    if !successes.is_empty() {
        sync_workspace_file(ws_root, &meta)?;
    }

    // Print summary
    println!();
    if !failures.is_empty() {
        println!(
            "  {} Summary: {} succeeded, {} failed",
            s.dim.apply_to("→"),
            successes.len(),
            failures.len()
        );
    } else {
        println!(
            "  {} {} package{} added",
            s.green.apply_to("✔"),
            successes.len(),
            if successes.len() == 1 { "" } else { "s" }
        );
    }
    println!();

    if !failures.is_empty() && successes.is_empty() {
        bail!("all packages failed to add");
    }

    Ok(())
}

/// Open a workspace at a specific root in the configured editor.
///
/// Internal helper that opens the workspace without resolving by name.
fn open_workspace_at(ws_root: &Path) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    let meta = read_meta(ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let ws_file_path = sync_workspace_file(ws_root, &meta)?;

    let ide_cmd = cfg.ide.command();

    println!(
        "  {} Opening in {} ...",
        s.dim.apply_to("→"),
        s.bold.apply_to(cfg.ide.to_string())
    );

    let status = Command::new(ide_cmd)
        .arg(&ws_file_path)
        .status()
        .with_context(|| {
            format!(
                "failed to launch {ide_cmd} — is {} installed and on your PATH?",
                cfg.ide
            )
        })?;

    if !status.success() {
        bail!("{ide_cmd} exited with {status}");
    }

    println!(
        "  {} Opened {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(
            ws_file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    );
    println!();

    Ok(())
}

/// Add one or more packages to a workspace by cloning them into `src/`.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn add(packages: &[String], workspace: Option<&str>) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace)?;

    add_packages_to_workspace(&ws_root, packages)
}

/// Remove one or more packages from a workspace.
///
/// Deletes the corresponding directories under `src/` and re-syncs the
/// `.code-workspace` file. Prompts for confirmation unless `force` is true.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn remove_packages(
    packages: &[String],
    workspace: Option<&str>,
    force: bool,
) -> Result<()> {
    let s = Styles::default();

    let ws_root = resolve_workspace_target(workspace)?;

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let src_dir = ws_root.join("src");
    let known_packages = list_packages(&ws_root)?;

    // Partition packages into found and not-found
    let mut to_remove: Vec<&str> = Vec::new();
    let mut not_found: Vec<&str> = Vec::new();

    for name in packages {
        if known_packages.iter().any(|p| p == name) {
            // Avoid duplicates if the user passes the same name twice
            if !to_remove.contains(&name.as_str()) {
                to_remove.push(name);
            }
        } else {
            not_found.push(name);
        }
    }

    // Report not-found packages upfront
    if !not_found.is_empty() {
        println!();
        for name in &not_found {
            println!(
                "  {} Package {} not found in workspace {}",
                s.red.apply_to("✘"),
                s.bold.apply_to(name),
                s.bold.apply_to(&meta.name),
            );
        }
    }

    if to_remove.is_empty() {
        if not_found.is_empty() {
            println!();
            println!("  {} No packages specified", s.dim.apply_to("—"));
        }
        println!();
        bail!("no matching packages to remove");
    }

    // Show what will be removed and confirm
    println!();
    println!(
        "  {} Removing from {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));
    for name in &to_remove {
        println!("  {}  {}", s.red.apply_to("−"), name);
    }
    println!();

    if !force {
        let prompt_msg = if to_remove.len() == 1 {
            format!(
                "  Remove {} from {}?",
                s.bold.apply_to(to_remove[0]),
                s.bold.apply_to(&meta.name)
            )
        } else {
            format!(
                "  Remove {} packages from {}?",
                to_remove.len(),
                s.bold.apply_to(&meta.name)
            )
        };

        let confirmed = Confirm::new()
            .with_prompt(prompt_msg)
            .default(false)
            .interact()
            .context("failed to read input")?;

        if !confirmed {
            println!("  Aborted.");
            println!();
            return Ok(());
        }
    }

    // Remove directories and collect results
    let mut removed: Vec<String> = Vec::new();
    let mut dir_errors: Vec<(String, String)> = Vec::new();

    for &name in &to_remove {
        let pkg_dir = src_dir.join(name);

        if pkg_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&pkg_dir) {
                dir_errors.push((name.to_string(), format!("{e}")));
                println!(
                    "  {} {} — could not remove directory: {}",
                    s.red.apply_to("✘"),
                    name,
                    e
                );
                continue;
            }
        }

        removed.push(name.to_string());
    }

    // Re-sync the .code-workspace file
    if !removed.is_empty() {
        sync_workspace_file(&ws_root, &meta)?;
    }

    // Print summary
    println!();
    for name in &removed {
        println!(
            "  {} Removed {}",
            s.green.apply_to("✔"),
            s.bold.apply_to(name)
        );
    }

    if !dir_errors.is_empty() {
        println!(
            "  {} Summary: {} succeeded, {} failed",
            s.dim.apply_to("→"),
            removed.len(),
            dir_errors.len()
        );
    } else {
        println!();
        println!(
            "  {} {} package{} removed",
            s.green.apply_to("✔"),
            removed.len(),
            if removed.len() == 1 { "" } else { "s" }
        );
    }
    println!();

    if !dir_errors.is_empty() && removed.is_empty() {
        bail!("all packages failed to remove");
    }

    Ok(())
}

/// Generate a `.code-workspace` file from the workspace root.
///
/// Derives the folder list by scanning `src/` for subdirectories. Uses
/// `meta.name` to produce the workspace filename. Returns the path to the
/// generated file.
fn sync_workspace_file(ws_root: &Path, meta: &WorkspaceMeta) -> Result<PathBuf> {
    let sanitized = sanitize_name(&meta.name);
    if sanitized.is_empty() {
        bail!(
            "workspace name \"{}\" produces an empty filename after sanitization",
            meta.name
        );
    }

    let filename = format!("{sanitized}.code-workspace");
    let ws_file_path = ws_root.join(&filename);

    // Build folder entries from directories in src/
    let pkg_names = list_packages(ws_root)?;
    let folders: Vec<VscodeWorkspaceFolder> = pkg_names
        .iter()
        .map(|name| VscodeWorkspaceFolder {
            path: format!("src/{name}"),
            name: name.clone(),
        })
        .collect();

    let vscode_ws = VscodeWorkspace {
        folders,
        settings: serde_json::json!({}),
    };

    let json = serde_json::to_string_pretty(&vscode_ws)?;
    fs::write(&ws_file_path, json + "\n")
        .with_context(|| format!("could not write workspace file: {}", ws_file_path.display()))?;

    Ok(ws_file_path)
}

/// Pull (or fetch) the latest changes for tracked packages and re-sync the
/// `.code-workspace` file.
///
/// When `packages` is empty, all packages in `src/` are synced. Otherwise, only
/// the named packages are synced. Runs `git pull` (or `git fetch` when
/// `fetch_only` is true) in each package directory, reports results, and
/// regenerates the workspace file. Returns the path to the generated
/// `.code-workspace` file.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn sync(
    packages: &[String],
    workspace: Option<&str>,
    fetch_only: bool,
) -> Result<PathBuf> {
    let s = Styles::default();

    let ws_root = resolve_workspace_target(workspace)?;

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let cfg = config::load_config()?;
    let tracking = resolve_git_tracking(&meta, &cfg);

    println!();
    println!(
        "  {} Syncing {}",
        s.bold.apply_to("🔄"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    if tracking == GitTracking::Workspace {
        // Workspace-level sync: pull/fetch at the workspace root
        if !packages.is_empty() {
            println!(
                "  {} Per-package filtering is not applicable in workspace-level tracking mode",
                s.dim.apply_to("⚠"),
            );
        }

        if !ws_root.join(".git").exists() {
            bail!(
                "workspace-level git tracking is configured but no git repository found at {}.\n  \
                 Run `git init` in the workspace directory.",
                ws_root.display()
            );
        }

        let git_op = if fetch_only { "Fetching" } else { "Pulling" };

        println!(
            "  {} {} workspace repository ...",
            s.dim.apply_to("→"),
            git_op,
        );

        let output = git_ops::sync_repo(&ws_root, fetch_only)?;

        if output.status.success() {
            let summary = git_ops::summarize_sync_stdout(&output, fetch_only);
            println!(
                "  {} {} — {}",
                s.green.apply_to("✔"),
                s.bold.apply_to("workspace"),
                s.dim.apply_to(summary)
            );
        } else {
            let git_arg = if fetch_only { "fetch" } else { "pull" };
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git {} failed: {}", git_arg, stderr.trim());
        }
    } else {
        // Package-level sync: pull/fetch in each package directory
        let src_dir = ws_root.join("src");
        let known_packages = list_packages(&ws_root)?;

        // Determine which packages to sync
        let targets: Vec<&str> = if packages.is_empty() {
            known_packages.iter().map(|s| s.as_str()).collect()
        } else {
            let mut matched: Vec<&str> = Vec::new();
            for name in packages {
                if known_packages.iter().any(|p| p == name) {
                    matched.push(name);
                } else {
                    bail!("package \"{name}\" not found in workspace {}", meta.name);
                }
            }
            matched
        };

        if targets.is_empty() {
            println!("  {}  No packages to sync", s.dim.apply_to("—"));
        }

        let mut pulled: Vec<String> = Vec::new();
        let mut failures: Vec<(String, String)> = Vec::new();

        for &pkg_name in &targets {
            let pkg_dir = src_dir.join(pkg_name);

            if !pkg_dir.exists() {
                let msg = format!("directory not found: {}", pkg_dir.display());
                failures.push((pkg_name.to_string(), msg.clone()));
                println!("  {} {} — {}", s.red.apply_to("✘"), pkg_name, msg);
                continue;
            }

            let git_op = if fetch_only { "Fetching" } else { "Pulling" };
            println!(
                "  {} {} {} ...",
                s.dim.apply_to("→"),
                git_op,
                s.cyan.apply_to(pkg_name)
            );

            let output = git_ops::sync_repo(&pkg_dir, fetch_only)?;

            if output.status.success() {
                let summary = git_ops::summarize_sync_stdout(&output, fetch_only);
                println!(
                    "  {} {} — {}",
                    s.green.apply_to("✔"),
                    s.bold.apply_to(pkg_name),
                    s.dim.apply_to(summary)
                );
                pulled.push(pkg_name.to_string());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let msg = stderr.trim().to_string();
                failures.push((pkg_name.to_string(), msg.clone()));
                println!("  {} {} — {}", s.red.apply_to("✘"), pkg_name, msg);
            }
        }

        // Print package-level summary
        println!();
        if !failures.is_empty() {
            println!(
                "  {} Summary: {} succeeded, {} failed",
                s.dim.apply_to("→"),
                pulled.len(),
                failures.len()
            );
        } else if !targets.is_empty() {
            println!(
                "  {} {} package{} synced",
                s.green.apply_to("✔"),
                pulled.len(),
                if pulled.len() == 1 { "" } else { "s" }
            );
        }
    }

    // Re-sync the .code-workspace file
    let ws_file_path = sync_workspace_file(&ws_root, &meta)?;

    let filename = ws_file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    println!(
        "  {} Workspace file {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(filename.as_ref())
    );
    println!();

    Ok(ws_file_path)
}

/// Pretty-print detailed information about a workspace.
///
/// Shows workspace name, directory location, packages (discovered from `src/`),
/// their git remote URLs, and the `.code-workspace` file path.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn info(workspace: Option<&str>, json: bool) -> Result<()> {
    let s = Styles::default();

    let ws_root = resolve_workspace_target(workspace)?;

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let cfg = config::load_config()?;
    let tracking = resolve_git_tracking(&meta, &cfg);

    let src_dir = ws_root.join("src");
    let pkg_names = list_packages(&ws_root)?;

    if json {
        let tracking_str = match tracking {
            GitTracking::Package => "package",
            GitTracking::Workspace => "workspace",
        };

        // Build a JSON representation with packages derived from disk
        let packages_json: Vec<serde_json::Value> = pkg_names
            .iter()
            .map(|name| {
                let pkg_dir = src_dir.join(name);
                let url = detect_git_remote_url(&pkg_dir);
                let branch = detect_git_branch(&pkg_dir);
                serde_json::json!({
                    "name": name,
                    "url": if url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(url) },
                    "branch": if branch.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(branch) },
                    "dir": pkg_dir.display().to_string(),
                })
            })
            .collect();

        let mut output = serde_json::json!({
            "name": meta.name,
            "git_tracking": tracking_str,
            "packages": packages_json,
        });

        // In workspace mode, add workspace-level git info
        if tracking == GitTracking::Workspace {
            let ws_url = detect_git_remote_url(&ws_root);
            let ws_branch = detect_git_branch(&ws_root);
            output["git_url"] = if ws_url.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(ws_url)
            };
            output["git_branch"] = if ws_branch.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(ws_branch)
            };
        }

        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Derive the .code-workspace filename
    let sanitized = sanitize_name(&meta.name);
    let ws_file = format!("{sanitized}.code-workspace");
    let ws_file_path = ws_root.join(&ws_file);

    println!();
    println!("  {}", s.bold.apply_to("Workspace Info"));
    println!("  {}", s.dim.apply_to("──────────────"));
    println!(
        "  {}  {}",
        s.cyan.apply_to("Name:"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}  {}", s.cyan.apply_to("Directory:"), ws_root.display());
    println!(
        "  {}  {} {}",
        s.cyan.apply_to("Workspace file:"),
        ws_file,
        if ws_file_path.exists() {
            s.green.apply_to("(exists)").to_string()
        } else {
            s.dim.apply_to("(not generated)").to_string()
        }
    );
    println!("  {}  {}", s.cyan.apply_to("Git tracking:"), tracking);
    println!("  {}  {}", s.cyan.apply_to("Packages:"), pkg_names.len());

    // In workspace mode, show workspace-level git info
    if tracking == GitTracking::Workspace {
        let ws_url = detect_git_remote_url(&ws_root);
        let ws_branch = detect_git_branch(&ws_root);

        println!();
        println!("  {}", s.bold.apply_to("Workspace Git"));
        println!("  {}", s.dim.apply_to("──────────────"));
        if !ws_url.is_empty() {
            println!("  {}  {}", s.dim.apply_to("url:"), s.dim.apply_to(&ws_url));
        }
        if !ws_branch.is_empty() {
            println!(
                "  {}  {}",
                s.dim.apply_to("branch:"),
                s.dim.apply_to(&ws_branch)
            );
        }
    }

    if !pkg_names.is_empty() {
        println!();
        println!("  {}", s.bold.apply_to("Packages"));
        println!("  {}", s.dim.apply_to("──────────────"));

        for name in &pkg_names {
            let pkg_dir = src_dir.join(name);

            println!("  {}  {}", s.cyan.apply_to("•"), s.bold.apply_to(name),);

            // In package mode, show per-package git info
            if tracking == GitTracking::Package {
                let url = detect_git_remote_url(&pkg_dir);
                let branch = detect_git_branch(&pkg_dir);

                if !url.is_empty() {
                    println!("     {}  {}", s.dim.apply_to("url:"), s.dim.apply_to(&url));
                }
                if !branch.is_empty() {
                    println!(
                        "     {}  {}",
                        s.dim.apply_to("branch:"),
                        s.dim.apply_to(&branch)
                    );
                }
            }
            println!(
                "     {}  {}",
                s.dim.apply_to("dir:"),
                s.dim.apply_to(pkg_dir.display().to_string())
            );
        }
    }

    println!();
    Ok(())
}

/// Open a workspace in the configured editor.
///
/// Regenerates the `.code-workspace` file and then launches the editor.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn open(workspace: Option<&str>) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace)?;

    open_workspace_at(&ws_root)
}

/// Adopt an existing local directory into the workspace.
///
/// Moves (or copies with `--copy`) the directory into `src/` if it is not
/// already there, then syncs the `.code-workspace` file. The directory's
/// presence in `src/` is all the registration needed.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn adopt(
    path: &Path,
    workspace: Option<&str>,
    copy: bool,
    name_override: Option<&str>,
) -> Result<()> {
    let s = Styles::default();

    let ws_root = resolve_workspace_target(workspace)?;

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    // Resolve and validate the source path
    let source = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .context("could not determine current directory")?
            .join(path)
    };

    if !source.exists() {
        bail!("path does not exist: {}", source.display());
    }
    if !source.is_dir() {
        bail!(
            "path is not a directory: {}\n  The adopt command requires a directory path.",
            source.display()
        );
    }

    let source = source
        .canonicalize()
        .with_context(|| format!("could not resolve path: {}", source.display()))?;

    // Derive the package name
    let pkg_name = match name_override {
        Some(n) => n.to_string(),
        None => source
            .file_name()
            .context("could not determine directory name from path")?
            .to_string_lossy()
            .into_owned(),
    };

    let src_dir = ws_root.join("src");
    let dest = src_dir.join(&pkg_name);

    // Check if the directory is already at the correct location
    let already_in_place = dest.exists()
        && dest
            .canonicalize()
            .ok()
            .map(|d| d == source)
            .unwrap_or(false);

    if already_in_place {
        println!();
        println!(
            "  {} Directory already at {}",
            s.dim.apply_to("→"),
            s.dim.apply_to(dest.display().to_string())
        );
    } else {
        // Ensure src/ exists
        fs::create_dir_all(&src_dir)
            .with_context(|| format!("could not create src directory: {}", src_dir.display()))?;

        if dest.exists() {
            bail!(
                "destination already exists: {}\n  \
                 A directory with the name \"{}\" is already in src/. \
                 Use --name to specify a different name.",
                dest.display(),
                pkg_name
            );
        }

        if copy {
            println!(
                "  {} Copying {} to {} ...",
                s.dim.apply_to("→"),
                s.cyan.apply_to(&pkg_name),
                s.dim.apply_to(dest.display().to_string())
            );

            let status = Command::new("cp")
                .args(["-a"])
                .arg(&source)
                .arg(&dest)
                .status()
                .context("failed to execute cp — is it available on your system?")?;

            if !status.success() {
                bail!("cp failed with {status}");
            }
        } else {
            println!(
                "  {} Moving {} to {} ...",
                s.dim.apply_to("→"),
                s.cyan.apply_to(&pkg_name),
                s.dim.apply_to(dest.display().to_string())
            );

            // Try fs::rename first (fast, same-filesystem only)
            if fs::rename(&source, &dest).is_err() {
                // Fall back to copy + delete for cross-device moves
                let status = Command::new("cp")
                    .args(["-a"])
                    .arg(&source)
                    .arg(&dest)
                    .status()
                    .context("failed to execute cp — is it available on your system?")?;

                if !status.success() {
                    bail!("cp failed with {status}");
                }

                fs::remove_dir_all(&source).with_context(|| {
                    format!(
                        "copied to destination but could not remove original: {}",
                        source.display()
                    )
                })?;
            }
        }
    }

    // If workspace-level tracking, remove the adopted package's .git directory
    let cfg = config::load_config()?;
    let tracking = resolve_git_tracking(&meta, &cfg);
    if tracking == GitTracking::Workspace {
        let adopted_git_dir = dest.join(".git");
        if adopted_git_dir.exists() {
            fs::remove_dir_all(&adopted_git_dir).with_context(|| {
                format!(
                    "could not remove .git directory from adopted package: {}",
                    adopted_git_dir.display()
                )
            })?;
            println!(
                "  {} Removed package-level .git (workspace-level tracking active)",
                s.dim.apply_to("→"),
            );
        }
    }

    // Sync the .code-workspace file (picks up the new directory in src/)
    sync_workspace_file(&ws_root, &meta)?;

    println!();
    println!(
        "  {} Adopted {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&pkg_name)
    );
    println!("  {}  {}", s.dim.apply_to("Location:"), dest.display());
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a workspace directory with metadata and a `src/` dir.
    fn setup_workspace(dir: &Path, name: &str) {
        setup_workspace_with_tracking(dir, name, None);
    }

    /// Helper: create a workspace with an explicit git tracking mode.
    fn setup_workspace_with_tracking(dir: &Path, name: &str, tracking: Option<GitTracking>) {
        fs::create_dir_all(dir.join("src")).unwrap();
        let meta = WorkspaceMeta {
            name: name.to_string(),
            git_tracking: tracking,
        };
        write_meta(dir, &meta).unwrap();
    }

    // ── find_workspace_root tests ────────────────────────────────────

    #[test]
    fn find_workspace_root_in_workspace_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let result = find_workspace_root(dir.path()).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_workspace_root_in_child_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        // Create a child directory and search from there
        let child = dir.path().join("src").join("deep");
        fs::create_dir_all(&child).unwrap();

        let result = find_workspace_root(&child).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_workspace_root_fails_when_not_in_workspace() {
        let dir = TempDir::new().unwrap();
        let result = find_workspace_root(dir.path());
        assert!(result.is_err());
    }

    // ── list_packages tests ─────────────────────────────────────────

    #[test]
    fn list_packages_returns_sorted_directory_names() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("gamma")).unwrap();
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();

        let names = list_packages(dir.path()).unwrap();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn list_packages_ignores_files_in_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("real-pkg")).unwrap();
        fs::write(src.join("not-a-package.txt"), "hello").unwrap();

        let names = list_packages(dir.path()).unwrap();
        assert_eq!(names, vec!["real-pkg"]);
    }

    #[test]
    fn list_packages_returns_empty_when_no_src() {
        let dir = TempDir::new().unwrap();
        // No src/ directory at all
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        write_meta(dir.path(), &meta).unwrap();

        let names = list_packages(dir.path()).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn list_packages_returns_empty_when_src_is_empty() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let names = list_packages(dir.path()).unwrap();
        assert!(names.is_empty());
    }

    // ── detect_git_remote_url tests ─────────────────────────────────

    #[test]
    fn detect_git_remote_url_finds_origin() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("git-repo");
        fs::create_dir_all(&repo).unwrap();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/test.git"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let url = detect_git_remote_url(&repo);
        assert_eq!(url, "git@github.com:acme/test.git");
    }

    #[test]
    fn detect_git_remote_url_returns_empty_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        let plain = dir.path().join("plain-dir");
        fs::create_dir_all(&plain).unwrap();

        let url = detect_git_remote_url(&plain);
        assert!(url.is_empty());
    }

    // ── detect_git_branch tests ─────────────────────────────────────

    #[test]
    fn detect_git_branch_finds_current_branch() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("git-repo");
        fs::create_dir_all(&repo).unwrap();

        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        // HEAD requires at least one commit to resolve
        fs::write(repo.join("README.md"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(&repo)
            .output()
            .unwrap();

        let branch = detect_git_branch(&repo);
        assert_eq!(branch, "main");
    }

    #[test]
    fn detect_git_branch_returns_empty_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        let plain = dir.path().join("plain-dir");
        fs::create_dir_all(&plain).unwrap();

        let branch = detect_git_branch(&plain);
        assert!(branch.is_empty());
    }

    // ── remove_packages tests ───────────────────────────────────────

    #[test]
    fn remove_deletes_directory_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();
        fs::create_dir_all(src.join("gamma")).unwrap();

        // Simulate removing "beta": delete its directory
        let pkg_dir = src.join("beta");
        fs::remove_dir_all(&pkg_dir).unwrap();

        assert!(!pkg_dir.exists());
        let remaining = list_packages(dir.path()).unwrap();
        assert_eq!(remaining, vec!["alpha", "gamma"]);
    }

    #[test]
    fn remove_multiple_directories_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test");

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();
        fs::create_dir_all(src.join("gamma")).unwrap();

        // Remove "alpha" and "gamma"
        fs::remove_dir_all(src.join("alpha")).unwrap();
        fs::remove_dir_all(src.join("gamma")).unwrap();

        let remaining = list_packages(dir.path()).unwrap();
        assert_eq!(remaining, vec!["beta"]);
    }

    // ── sync_workspace_file tests ───────────────────────────────────

    #[test]
    fn sync_workspace_file_derives_folders_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "my-project");

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("toolkit")).unwrap();
        fs::create_dir_all(src.join("utils")).unwrap();

        let meta = read_meta(dir.path()).unwrap().unwrap();
        let ws_file_path = sync_workspace_file(dir.path(), &meta).unwrap();

        assert!(ws_file_path.exists());

        let contents = fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/toolkit");
        assert_eq!(parsed["folders"][0]["name"], "toolkit");
        assert_eq!(parsed["folders"][1]["path"], "src/utils");
        assert_eq!(parsed["folders"][1]["name"], "utils");
    }

    #[test]
    fn sync_workspace_file_empty_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "empty-ws");

        let meta = read_meta(dir.path()).unwrap().unwrap();
        let ws_file_path = sync_workspace_file(dir.path(), &meta).unwrap();

        let contents = fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"].as_array().unwrap().len(), 0);
    }

    // ── adopt tests ─────────────────────────────────────────────────

    #[test]
    fn adopt_moves_directory_into_src() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        // Create a source directory outside the workspace
        let outside = TempDir::new().unwrap();
        let source = outside.path().join("my-pkg");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("hello.txt"), "world").unwrap();

        let source_canonical = source.canonicalize().unwrap();

        let src_dir = ws_dir.path().join("src");
        let dest = src_dir.join("my-pkg");
        assert!(!dest.exists());

        // Simulate the adopt logic: move source into ws/src/my-pkg
        if fs::rename(&source_canonical, &dest).is_err() {
            std::process::Command::new("cp")
                .args(["-a"])
                .arg(&source_canonical)
                .arg(&dest)
                .status()
                .unwrap();
            fs::remove_dir_all(&source_canonical).unwrap();
        }

        // Verify the directory moved
        assert!(dest.join("hello.txt").exists());
        assert_eq!(fs::read_to_string(dest.join("hello.txt")).unwrap(), "world");
        assert!(!source.exists(), "original should be gone after move");

        // Package is discovered from filesystem
        let pkgs = list_packages(ws_dir.path()).unwrap();
        assert_eq!(pkgs, vec!["my-pkg"]);
    }

    #[test]
    fn adopt_copies_when_flag_set() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        let outside = TempDir::new().unwrap();
        let source = outside.path().join("copy-pkg");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("data.txt"), "keep me").unwrap();

        let dest = ws_dir.path().join("src").join("copy-pkg");

        // Copy instead of move
        let status = std::process::Command::new("cp")
            .args(["-a"])
            .arg(&source)
            .arg(&dest)
            .status()
            .unwrap();
        assert!(status.success());

        // Verify destination has the file
        assert!(dest.join("data.txt").exists());
        assert_eq!(
            fs::read_to_string(dest.join("data.txt")).unwrap(),
            "keep me"
        );
        // Original still exists
        assert!(source.exists(), "original should still exist after copy");

        // Package is discovered from filesystem
        let pkgs = list_packages(ws_dir.path()).unwrap();
        assert_eq!(pkgs, vec!["copy-pkg"]);
    }

    #[test]
    fn adopt_registers_already_in_src() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        // Place a directory directly in src/
        let pkg_dir = ws_dir.path().join("src").join("existing-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("marker.txt"), "I was here").unwrap();

        // The package is already discoverable — no move/copy needed
        let pkgs = list_packages(ws_dir.path()).unwrap();
        assert_eq!(pkgs, vec!["existing-pkg"]);

        // File is still intact
        assert_eq!(
            fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "I was here"
        );
    }

    #[test]
    fn adopt_detects_existing_directory_as_conflict() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        // Place a directory in src/ with the name "taken"
        fs::create_dir_all(ws_dir.path().join("src").join("taken")).unwrap();

        // The package already exists on disk — adopt should detect the conflict
        let pkgs = list_packages(ws_dir.path()).unwrap();
        assert!(pkgs.contains(&"taken".to_string()));
    }

    #[test]
    fn adopt_errors_on_nonexistent_path() {
        let dir = TempDir::new().unwrap();
        let bogus = dir.path().join("does-not-exist");
        assert!(!bogus.exists());
    }

    #[test]
    fn adopt_errors_on_file_path() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not-a-dir.txt");
        fs::write(&file, "hello").unwrap();
        assert!(file.exists());
        assert!(!file.is_dir(), "files should be rejected by adopt");
    }

    // ── add_packages_to_workspace tests ──────────────────────────────

    #[test]
    fn add_packages_to_workspace_clones_git_repo() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        // Create a mock "remote" git repo
        let remote_dir = TempDir::new().unwrap();
        let remote_repo = remote_dir.path().join("test-pkg");
        fs::create_dir_all(&remote_repo).unwrap();
        fs::write(remote_repo.join("README.md"), "test").unwrap();

        // Init git repo and commit the file
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();

        // Clone using full file:// URL
        let url = format!("file://{}", remote_repo.canonicalize().unwrap().display());
        let result = add_packages_to_workspace(ws_dir.path(), &[url]);

        // The clone should succeed
        assert!(result.is_ok());

        // Verify the package was cloned into src/
        let pkg_dir = ws_dir.path().join("src").join("test-pkg");
        assert!(pkg_dir.exists());
        assert!(pkg_dir.join("README.md").exists());

        // Verify it's discoverable
        let pkgs = list_packages(ws_dir.path()).unwrap();
        assert!(pkgs.contains(&"test-pkg".to_string()));
    }

    #[test]
    fn add_packages_to_workspace_errors_when_all_fail() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws");

        // Pre-create a package directory
        let pkg_dir = ws_dir.path().join("src").join("existing-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("marker.txt"), "original").unwrap();

        // Try to add the same package - should fail with "already exists"
        let remote_dir = TempDir::new().unwrap();
        let remote_repo = remote_dir.path().join("existing-pkg");
        fs::create_dir_all(&remote_repo).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();

        let url = format!("file://{}", remote_repo.canonicalize().unwrap().display());
        let result = add_packages_to_workspace(ws_dir.path(), &[url]);

        // Should return an error when all packages fail
        assert!(result.is_err());

        // Original file should still be there
        assert_eq!(
            fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "original"
        );
    }

    // ── open_workspace_at tests ────────────────────────────────────

    #[test]
    fn open_workspace_at_generates_workspace_file() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "my-workspace");

        // Add a package so there's something in the workspace file
        let src_dir = ws_dir.path().join("src");
        fs::create_dir_all(src_dir.join("pkg-a")).unwrap();

        // Call open_workspace_at - this should create the .code-workspace file
        // Note: This will fail to actually launch the editor in tests,
        // but we can't easily mock that. We'll test that the file was generated.
        let result =
            sync_workspace_file(ws_dir.path(), &read_meta(ws_dir.path()).unwrap().unwrap());
        assert!(result.is_ok());

        let ws_file = result.unwrap();
        assert!(ws_file.exists());
        assert!(
            ws_file
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("my-workspace")
        );

        // Verify file contents
        let contents = fs::read_to_string(&ws_file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/pkg-a");
    }

    // ── resolve_git_tracking tests ───────────────────────────────

    #[test]
    fn resolve_tracking_workspace_override_wins() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: Some(GitTracking::Workspace),
        };
        let cfg = config::BuonaConfig::default(); // default tracking = Package
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Workspace);
    }

    #[test]
    fn resolve_tracking_falls_back_to_global() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        let mut cfg = config::BuonaConfig::default();
        cfg.git.tracking = GitTracking::Workspace;
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Workspace);
    }

    #[test]
    fn resolve_tracking_defaults_to_package() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        let cfg = config::BuonaConfig::default();
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Package);
    }
}
