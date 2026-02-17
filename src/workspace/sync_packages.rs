//! Git sync (pull/fetch) workflow for workspace packages.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config;
use crate::config::GitTracking;
use crate::styles::Styles;

use super::git_ops;
use super::packages::list_package_names;
use super::types::{WorkspaceMeta, read_meta};
use super::workspace_file::sync_workspace_file;

/// Resolve the effective git tracking mode for a workspace.
///
/// Priority: workspace-level override > global config default > hardcoded Package.
fn resolve_git_tracking(meta: &WorkspaceMeta, cfg: &config::BuonaConfig) -> GitTracking {
    meta.git_tracking.unwrap_or(cfg.git.tracking)
}

/// Pull (or fetch) the latest changes for tracked packages and re-sync the
/// `.code-workspace` file.
///
/// When `packages` is empty, all packages in `src/` are synced. Otherwise, only
/// the named packages are synced. Runs `git pull` (or `git fetch` when
/// `fetch_only` is true) in each package directory, reports results, and
/// regenerates the workspace file. Returns the path to the generated
/// `.code-workspace` file.
pub(super) async fn sync_workspace(
    ws_root: &Path,
    packages: &[String],
    fetch_only: bool,
) -> Result<PathBuf> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let cfg = config::load_config().await?;
    let tracking = resolve_git_tracking(&meta, &cfg);

    println!();
    println!(
        "  {} Syncing {}",
        s.bold.apply_to("🔄"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    if tracking == GitTracking::Workspace {
        sync_workspace_level(ws_root, packages, fetch_only, &s).await?;
    } else {
        sync_package_level(ws_root, &meta, packages, fetch_only, &s).await?;
    }

    // Re-sync the .code-workspace file
    let ws_file_path = sync_workspace_file(ws_root, &meta).await?;

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

/// Workspace-level sync: pull/fetch at the workspace root.
async fn sync_workspace_level(
    ws_root: &Path,
    packages: &[String],
    fetch_only: bool,
    s: &Styles,
) -> Result<()> {
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

    let output = git_ops::sync_repo(ws_root, fetch_only).await?;

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

    Ok(())
}

/// Package-level sync: pull/fetch in each package directory.
async fn sync_package_level(
    ws_root: &Path,
    meta: &WorkspaceMeta,
    packages: &[String],
    fetch_only: bool,
    s: &Styles,
) -> Result<()> {
    let src_dir = ws_root.join("src");
    let known_packages = list_package_names(ws_root).await?;

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

        let output = git_ops::sync_repo(&pkg_dir, fetch_only).await?;

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

    Ok(())
}
