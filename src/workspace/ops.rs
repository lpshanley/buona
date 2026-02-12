//! Workspace operations — list, create, delete, add, remove, sync, and open.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::config;
use crate::styles::Styles;

use super::git::resolve_package_spec;
use super::types::{PackageEntry, WorkspaceMeta, WORKSPACE_FILE, read_meta, write_meta};
use super::vscode::{VscodeWorkspace, VscodeWorkspaceFolder, sanitize_name};

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
fn find_workspace_root(start: &Path) -> Result<PathBuf> {
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
pub(crate) fn create(path: &Path, name: Option<&str>) -> Result<()> {
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
        packages: Vec::new(),
    };
    write_meta(&target, &meta)?;

    // Auto-sync the .code-workspace file
    sync_workspace_file(&target, &meta)?;

    println!();
    println!(
        "  {} Created workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}  {}", s.dim.apply_to("Location:"), target.display());
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

/// Add one or more packages to a workspace by cloning them into `src/`.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn add(packages: &[String], workspace: Option<&str>) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let mut meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let src_dir = ws_root.join("src");

    println!();
    println!(
        "  {} Adding packages to {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    let mut successes: Vec<PackageEntry> = Vec::new();
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

        let output = Command::new("git")
            .arg("clone")
            .arg(&resolved.url)
            .arg(&dest)
            .output()
            .context("failed to execute git clone — is git installed?")?;

        if output.status.success() {
            println!(
                "  {} {}",
                s.green.apply_to("✔"),
                s.bold.apply_to(&resolved.name)
            );
            successes.push(PackageEntry {
                name: resolved.name,
                url: resolved.url,
            });
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            failures.push((spec.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
        }
    }

    // Update workspace metadata with successfully cloned packages
    if !successes.is_empty() {
        meta.packages.extend(successes.iter().cloned());
        write_meta(&ws_root, &meta)?;

        // Auto-sync the .code-workspace file
        sync_workspace_file(&ws_root, &meta)?;
    }

    // Print summary
    println!();
    if !failures.is_empty() {
        println!("  {} added, {} failed", successes.len(), failures.len());
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

/// Remove one or more packages from a workspace.
///
/// Removes the package entries from `buona.workspace.json` and deletes the
/// corresponding directories under `src/`. Prompts for confirmation unless
/// `force` is true.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn remove_packages(packages: &[String], workspace: Option<&str>, force: bool) -> Result<()> {
    let s = Styles::default();

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let mut meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let src_dir = ws_root.join("src");

    // Partition packages into found (with their index) and not-found
    let mut to_remove: Vec<(usize, &PackageEntry)> = Vec::new();
    let mut not_found: Vec<&str> = Vec::new();

    for name in packages {
        if let Some(idx) = meta.packages.iter().position(|p| p.name == *name) {
            // Avoid duplicates if the user passes the same name twice
            if !to_remove.iter().any(|(i, _)| *i == idx) {
                to_remove.push((idx, &meta.packages[idx]));
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
    for (_, pkg) in &to_remove {
        println!("  {}  {}", s.red.apply_to("−"), pkg.name);
    }
    println!();

    if !force {
        let prompt_msg = if to_remove.len() == 1 {
            format!(
                "  Remove {} from {}?",
                s.bold.apply_to(&to_remove[0].1.name),
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

    // Collect indices to remove (sorted in reverse to avoid shifting)
    let mut indices: Vec<usize> = to_remove.iter().map(|(i, _)| *i).collect();
    indices.sort_unstable();
    indices.reverse();

    for &idx in &indices {
        let pkg = &meta.packages[idx];
        let pkg_dir = src_dir.join(&pkg.name);
        let pkg_name = pkg.name.clone();

        if pkg_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&pkg_dir) {
                dir_errors.push((pkg_name.clone(), format!("{e}")));
                println!(
                    "  {} {} — could not remove directory: {}",
                    s.red.apply_to("✘"),
                    pkg_name,
                    e
                );
                continue;
            }
        }

        // Remove from metadata
        meta.packages.remove(idx);
        removed.push(pkg_name);
    }

    // Save updated metadata
    if !removed.is_empty() {
        write_meta(&ws_root, &meta)?;

        // Auto-sync the .code-workspace file
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
            "  {} removed, {} failed",
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

/// Generate a `.code-workspace` file from the given workspace root and metadata.
///
/// This is the core sync logic shared by `sync()` and other operations that
/// modify metadata (`create`, `add`, `remove`). Returns the path to the
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

    // Build folder entries from tracked packages
    let folders: Vec<VscodeWorkspaceFolder> = meta
        .packages
        .iter()
        .map(|pkg| VscodeWorkspaceFolder {
            path: format!("src/{}", pkg.name),
            name: pkg.name.clone(),
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
/// When `packages` is empty, all tracked packages are synced. Otherwise, only
/// the named packages are synced. Runs `git pull` (or `git fetch` when
/// `fetch_only` is true) in each package directory, reports results, and
/// regenerates the workspace file. Returns the path to the generated
/// `.code-workspace` file.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn sync(packages: &[String], workspace: Option<&str>, fetch_only: bool) -> Result<PathBuf> {
    let s = Styles::default();

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let src_dir = ws_root.join("src");

    // Determine which packages to sync
    let targets: Vec<&PackageEntry> = if packages.is_empty() {
        meta.packages.iter().collect()
    } else {
        let mut matched: Vec<&PackageEntry> = Vec::new();
        for name in packages {
            match meta.packages.iter().find(|p| p.name == *name) {
                Some(pkg) => matched.push(pkg),
                None => bail!("package \"{name}\" not found in workspace {}", meta.name),
            }
        }
        matched
    };

    println!();
    println!(
        "  {} Syncing {}",
        s.bold.apply_to("🔄"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    if targets.is_empty() {
        println!("  {}  No packages to sync", s.dim.apply_to("—"));
    }

    let mut pulled: Vec<String> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for pkg in &targets {
        let pkg_dir = src_dir.join(&pkg.name);

        if !pkg_dir.exists() {
            let msg = format!("directory not found: {}", pkg_dir.display());
            failures.push((pkg.name.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), pkg.name, msg);
            continue;
        }

        let git_op = if fetch_only { "Fetching" } else { "Pulling" };
        println!(
            "  {} {} {} ...",
            s.dim.apply_to("→"),
            git_op,
            s.cyan.apply_to(&pkg.name)
        );

        let git_arg = if fetch_only { "fetch" } else { "pull" };
        let output = Command::new("git")
            .arg(git_arg)
            .current_dir(&pkg_dir)
            .output()
            .with_context(|| format!("failed to execute git {git_arg} — is git installed?"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let fallback = if fetch_only { "done" } else { "up to date" };
            let summary = stdout.lines().next().unwrap_or(fallback).trim();
            let summary = if summary.is_empty() { fallback } else { summary };
            println!(
                "  {} {} — {}",
                s.green.apply_to("✔"),
                s.bold.apply_to(&pkg.name),
                s.dim.apply_to(summary)
            );
            pulled.push(pkg.name.clone());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            failures.push((pkg.name.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), pkg.name, msg);
        }
    }

    // Re-sync the .code-workspace file
    let ws_file_path = sync_workspace_file(&ws_root, &meta)?;

    // Print summary
    let verb = if fetch_only { "fetched" } else { "pulled" };
    println!();
    if !failures.is_empty() {
        println!("  {} {verb}, {} failed", pulled.len(), failures.len());
    } else if !targets.is_empty() {
        println!(
            "  {} {} package{} synced",
            s.green.apply_to("✔"),
            pulled.len(),
            if pulled.len() == 1 { "" } else { "s" }
        );
    }

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

/// Open a workspace in the configured editor.
///
/// Regenerates the `.code-workspace` file and then launches the editor.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn open(workspace: Option<&str>) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    // Resolve the workspace root and regenerate the .code-workspace file
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let ws_file_path = sync_workspace_file(&ws_root, &meta)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── find_workspace_root tests ────────────────────────────────────

    #[test]
    fn find_workspace_root_in_workspace_dir() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = find_workspace_root(dir.path()).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_workspace_root_in_child_dir() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

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

    // ── sync tests ──────────────────────────────────────────────────

    // ── remove_packages tests ───────────────────────────────────────

    #[test]
    fn remove_packages_removes_from_metadata() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: vec![
                PackageEntry {
                    name: "alpha".to_string(),
                    url: "git@github.com:acme/alpha.git".to_string(),
                },
                PackageEntry {
                    name: "beta".to_string(),
                    url: "git@github.com:acme/beta.git".to_string(),
                },
                PackageEntry {
                    name: "gamma".to_string(),
                    url: "git@github.com:acme/gamma.git".to_string(),
                },
            ],
        };
        write_meta(dir.path(), &meta).unwrap();

        // Create package directories
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();
        fs::create_dir_all(src.join("gamma")).unwrap();

        // Simulate dropping "beta" with force (no confirmation prompt)
        // We call the internal logic directly since drop_packages uses
        // find_workspace_from_cwd / find_workspace which needs the global config.
        // Instead, test the metadata manipulation and dir removal directly.
        let mut meta = read_meta(dir.path()).unwrap().unwrap();
        let idx = meta.packages.iter().position(|p| p.name == "beta").unwrap();
        let pkg_dir = src.join("beta");
        fs::remove_dir_all(&pkg_dir).unwrap();
        meta.packages.remove(idx);
        write_meta(dir.path(), &meta).unwrap();

        let updated = read_meta(dir.path()).unwrap().unwrap();
        assert_eq!(updated.packages.len(), 2);
        assert_eq!(updated.packages[0].name, "alpha");
        assert_eq!(updated.packages[1].name, "gamma");
        assert!(!pkg_dir.exists());
    }

    #[test]
    fn remove_packages_removes_multiple_from_metadata() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: vec![
                PackageEntry {
                    name: "alpha".to_string(),
                    url: "git@github.com:acme/alpha.git".to_string(),
                },
                PackageEntry {
                    name: "beta".to_string(),
                    url: "git@github.com:acme/beta.git".to_string(),
                },
                PackageEntry {
                    name: "gamma".to_string(),
                    url: "git@github.com:acme/gamma.git".to_string(),
                },
            ],
        };
        write_meta(dir.path(), &meta).unwrap();

        let src = dir.path().join("src");
        fs::create_dir_all(src.join("alpha")).unwrap();
        fs::create_dir_all(src.join("beta")).unwrap();
        fs::create_dir_all(src.join("gamma")).unwrap();

        // Remove "alpha" and "gamma" (indices 0 and 2), reverse order to avoid shifting
        let mut meta = read_meta(dir.path()).unwrap().unwrap();
        let mut indices: Vec<usize> = vec![0, 2];
        indices.sort_unstable();
        indices.reverse();
        for idx in indices {
            let pkg_dir = src.join(&meta.packages[idx].name);
            fs::remove_dir_all(&pkg_dir).unwrap();
            meta.packages.remove(idx);
        }
        write_meta(dir.path(), &meta).unwrap();

        let updated = read_meta(dir.path()).unwrap().unwrap();
        assert_eq!(updated.packages.len(), 1);
        assert_eq!(updated.packages[0].name, "beta");
        assert!(!src.join("alpha").exists());
        assert!(src.join("beta").exists());
        assert!(!src.join("gamma").exists());
    }

    #[test]
    fn remove_packages_handles_missing_directory_gracefully() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: vec![PackageEntry {
                name: "phantom".to_string(),
                url: "git@github.com:acme/phantom.git".to_string(),
            }],
        };
        write_meta(dir.path(), &meta).unwrap();

        // Don't create the src/phantom directory — it's tracked but not on disk
        let src = dir.path().join("src");
        let pkg_dir = src.join("phantom");
        assert!(!pkg_dir.exists());

        // The metadata removal should still succeed
        let mut meta = read_meta(dir.path()).unwrap().unwrap();
        meta.packages.remove(0);
        write_meta(dir.path(), &meta).unwrap();

        let updated = read_meta(dir.path()).unwrap().unwrap();
        assert!(updated.packages.is_empty());
    }

    // ── sync tests ──────────────────────────────────────────────────

    #[test]
    fn sync_creates_code_workspace_file() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            packages: vec![
                PackageEntry {
                    name: "toolkit".to_string(),
                    url: "git@github.com:acme/toolkit.git".to_string(),
                },
                PackageEntry {
                    name: "utils".to_string(),
                    url: "git@github.com:acme/utils.git".to_string(),
                },
            ],
        };
        write_meta(dir.path(), &meta).unwrap();

        // We can't call sync() directly because it tries to resolve the workspace
        // via config. Instead, test the pieces: sanitize + write.
        let sanitized = sanitize_name(&meta.name);
        assert_eq!(sanitized, "my-project");

        let filename = format!("{sanitized}.code-workspace");
        let ws_file_path = dir.path().join(&filename);

        let folders: Vec<VscodeWorkspaceFolder> = meta
            .packages
            .iter()
            .map(|pkg| VscodeWorkspaceFolder {
                path: format!("src/{}", pkg.name),
                name: pkg.name.clone(),
            })
            .collect();

        let vscode_ws = VscodeWorkspace {
            folders,
            settings: serde_json::json!({}),
        };

        let json = serde_json::to_string_pretty(&vscode_ws).unwrap();
        fs::write(&ws_file_path, &json).unwrap();

        // Verify the file was written correctly
        assert!(ws_file_path.exists());

        let contents = fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/toolkit");
        assert_eq!(parsed["folders"][0]["name"], "toolkit");
        assert_eq!(parsed["folders"][1]["path"], "src/utils");
        assert_eq!(parsed["folders"][1]["name"], "utils");
    }
}
