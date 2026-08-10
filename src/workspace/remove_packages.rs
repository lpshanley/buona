//! Package removal workflow for workspaces.

use std::path::Path;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::styles::Styles;

use super::packages::list_package_names;
use super::types::read_meta;
use super::workspace_file::sync_workspace_file;

/// Remove one or more packages from a workspace.
///
/// Deletes the corresponding directories under `src/` and re-syncs the
/// `.code-workspace` file. Prompts for confirmation unless `force` is true.
pub(super) async fn remove_packages_from_workspace(
    ws_root: &Path,
    packages: &[String],
    force: bool,
) -> Result<()> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let src_dir = ws_root.join("src");
    let known_packages = list_package_names(ws_root).await?;

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
        crate::textln!();
        for name in &not_found {
            crate::textln!(
                "  {} Package {} not found in workspace {}",
                s.red.apply_to("✘"),
                s.bold.apply_to(name),
                s.bold.apply_to(&meta.name),
            );
        }
    }

    if to_remove.is_empty() {
        if not_found.is_empty() {
            crate::textln!();
            crate::textln!("  {} No packages specified", s.dim.apply_to("—"));
        }
        crate::textln!();
        bail!("no matching packages to remove");
    }

    // Show what will be removed and confirm
    crate::textln!();
    crate::textln!(
        "  {} Removing from {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    crate::textln!("  {}", s.dim.apply_to("───────────────────────────"));
    for name in &to_remove {
        crate::textln!("  {}  {}", s.red.apply_to("−"), name);
    }
    crate::textln!();

    if !force && crate::output::is_non_interactive() {
        bail!("package removal requires confirmation; re-run with --yes");
    }

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
            crate::textln!("  Aborted.");
            crate::textln!();
            return Ok(());
        }
    }

    // Remove directories and collect results
    let mut removed: Vec<String> = Vec::new();
    let mut dir_errors: Vec<(String, String)> = Vec::new();

    for &name in &to_remove {
        let pkg_dir = src_dir.join(name);

        if pkg_dir.exists()
            && let Err(e) = tokio::fs::remove_dir_all(&pkg_dir).await
        {
            dir_errors.push((name.to_string(), format!("{e}")));
            crate::textln!(
                "  {} {} — could not remove directory: {}",
                s.red.apply_to("✘"),
                name,
                e
            );
            continue;
        }

        removed.push(name.to_string());
    }

    // Re-sync the .code-workspace file
    if !removed.is_empty() {
        sync_workspace_file(ws_root, &meta).await?;
    }

    // Print summary
    crate::textln!();
    for name in &removed {
        crate::textln!(
            "  {} Removed {}",
            s.green.apply_to("✔"),
            s.bold.apply_to(name)
        );
    }

    if !dir_errors.is_empty() {
        crate::textln!(
            "  {} Summary: {} succeeded, {} failed",
            s.dim.apply_to("→"),
            removed.len(),
            dir_errors.len()
        );
    } else {
        crate::textln!();
        crate::textln!(
            "  {} {} package{} removed",
            s.green.apply_to("✔"),
            removed.len(),
            if removed.len() == 1 { "" } else { "s" }
        );
    }
    crate::textln!();

    if !dir_errors.is_empty() && removed.is_empty() {
        bail!("all packages failed to remove");
    }

    Ok(())
}
