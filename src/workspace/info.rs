//! Workspace info display (human-readable and JSON).

use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::config::GitTracking;
use crate::styles::Styles;

use super::git_ops;
use super::packages::list_package_names;
use super::types::{WorkspaceMeta, read_meta};
use super::vscode::sanitize_name;

/// Detect the git remote origin URL for a directory, if it is a git repo.
async fn detect_git_remote_url(dir: &Path) -> String {
    git_ops::detect_remote_url(dir).await
}

/// Detect the current git branch for a directory.
async fn detect_git_branch(dir: &Path) -> String {
    git_ops::detect_branch(dir).await
}

/// Pretty-print detailed information about a workspace.
///
/// Shows workspace name, directory location, packages (discovered from `src/`),
/// their git remote URLs, and the `.code-workspace` file path.
pub(super) async fn show_info(ws_root: &Path, json: bool) -> Result<()> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let cfg = config::load_config().await?;
    let tracking = meta.effective_tracking(&cfg);

    let src_dir = ws_root.join("src");
    let pkg_names = list_package_names(ws_root).await?;

    if json {
        return print_json(ws_root, &meta, tracking, &src_dir, &pkg_names).await;
    }

    // Derive the .code-workspace filename
    let sanitized = sanitize_name(&meta.name);
    let ws_file = format!("{sanitized}.code-workspace");
    let ws_file_path = ws_root.join(&ws_file);

    crate::textln!();
    crate::textln!("  {}", s.bold.apply_to("Workspace Info"));
    crate::textln!("  {}", s.dim.apply_to("──────────────"));
    crate::textln!(
        "  {}  {}",
        s.cyan.apply_to("Name:"),
        s.bold.apply_to(&meta.name)
    );
    crate::textln!("  {}  {}", s.cyan.apply_to("Directory:"), ws_root.display());
    crate::textln!(
        "  {}  {} {}",
        s.cyan.apply_to("Workspace file:"),
        ws_file,
        if ws_file_path.exists() {
            s.green.apply_to("(exists)").to_string()
        } else {
            s.dim.apply_to("(not generated)").to_string()
        }
    );
    crate::textln!("  {}  {}", s.cyan.apply_to("Git tracking:"), tracking);
    crate::textln!("  {}  {}", s.cyan.apply_to("Packages:"), pkg_names.len());

    // In workspace mode, show workspace-level git info
    if tracking == GitTracking::Workspace {
        let ws_url = detect_git_remote_url(ws_root).await;
        let ws_branch = detect_git_branch(ws_root).await;

        crate::textln!();
        crate::textln!("  {}", s.bold.apply_to("Workspace Git"));
        crate::textln!("  {}", s.dim.apply_to("──────────────"));
        if !ws_url.is_empty() {
            crate::textln!("  {}  {}", s.dim.apply_to("url:"), s.dim.apply_to(&ws_url));
        }
        if !ws_branch.is_empty() {
            crate::textln!(
                "  {}  {}",
                s.dim.apply_to("branch:"),
                s.dim.apply_to(&ws_branch)
            );
        }
    }

    if !pkg_names.is_empty() {
        crate::textln!();
        crate::textln!("  {}", s.bold.apply_to("Packages"));
        crate::textln!("  {}", s.dim.apply_to("──────────────"));

        for name in &pkg_names {
            let pkg_dir = src_dir.join(name);

            crate::textln!("  {}  {}", s.cyan.apply_to("•"), s.bold.apply_to(name),);

            // In package mode, show per-package git info
            if tracking == GitTracking::Package {
                let url = detect_git_remote_url(&pkg_dir).await;
                let branch = detect_git_branch(&pkg_dir).await;

                if !url.is_empty() {
                    crate::textln!("     {}  {}", s.dim.apply_to("url:"), s.dim.apply_to(&url));
                }
                if !branch.is_empty() {
                    crate::textln!(
                        "     {}  {}",
                        s.dim.apply_to("branch:"),
                        s.dim.apply_to(&branch)
                    );
                }
            }
            crate::textln!(
                "     {}  {}",
                s.dim.apply_to("dir:"),
                s.dim.apply_to(pkg_dir.display().to_string())
            );
        }
    }

    crate::textln!();
    Ok(())
}

/// Print workspace info as JSON.
async fn print_json(
    ws_root: &Path,
    meta: &WorkspaceMeta,
    tracking: GitTracking,
    src_dir: &Path,
    pkg_names: &[String],
) -> Result<()> {
    let tracking_str = match tracking {
        GitTracking::Package => "package",
        GitTracking::Workspace => "workspace",
    };

    let mut packages_json: Vec<serde_json::Value> = Vec::new();
    for name in pkg_names {
        let pkg_dir = src_dir.join(name);
        let url = detect_git_remote_url(&pkg_dir).await;
        let branch = detect_git_branch(&pkg_dir).await;
        packages_json.push(serde_json::json!({
            "name": name,
            "url": if url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(url) },
            "branch": if branch.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(branch) },
            "dir": pkg_dir.display().to_string(),
        }));
    }

    let sanitized = sanitize_name(&meta.name);
    let workspace_file = ws_root.join(format!("{sanitized}.code-workspace"));
    let mut output = serde_json::json!({
        "name": meta.name,
        "directory": ws_root,
        "metadata_file": ws_root.join("buona.workspace.json"),
        "workspace_file": workspace_file,
        "workspace_file_exists": workspace_file.is_file(),
        "git_tracking": tracking_str,
        "packages": packages_json,
    });

    if tracking == GitTracking::Workspace {
        let ws_url = detect_git_remote_url(ws_root).await;
        let ws_branch = detect_git_branch(ws_root).await;
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

    crate::output::print_json(&output)
}
