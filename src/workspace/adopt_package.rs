//! Adopt an existing local directory into a workspace.

use std::env;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use crate::config;
use crate::config::GitTracking;
use crate::styles::Styles;

use super::types::{WorkspaceMeta, read_meta};
use super::workspace_file::sync_workspace_file;

/// Resolve the effective git tracking mode for a workspace.
fn resolve_git_tracking(meta: &WorkspaceMeta, cfg: &config::BuonaConfig) -> GitTracking {
    meta.git_tracking.unwrap_or(cfg.git.tracking)
}

/// Adopt an existing local directory into the workspace.
///
/// Moves (or copies with `copy`) the directory into `src/` if it is not
/// already there, then syncs the `.code-workspace` file. The directory's
/// presence in `src/` is all the registration needed.
pub(super) async fn adopt_into_workspace(
    ws_root: &Path,
    path: &Path,
    copy: bool,
    name_override: Option<&str>,
) -> Result<()> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
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
        tokio::fs::create_dir_all(&src_dir)
            .await
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
                .await
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

            // Try tokio::fs::rename first (fast, same-filesystem only)
            if tokio::fs::rename(&source, &dest).await.is_err() {
                // Fall back to copy + delete for cross-device moves
                let status = Command::new("cp")
                    .args(["-a"])
                    .arg(&source)
                    .arg(&dest)
                    .status()
                    .await
                    .context("failed to execute cp — is it available on your system?")?;

                if !status.success() {
                    bail!("cp failed with {status}");
                }

                tokio::fs::remove_dir_all(&source).await.with_context(|| {
                    format!(
                        "copied to destination but could not remove original: {}",
                        source.display()
                    )
                })?;
            }
        }
    }

    // If workspace-level tracking, remove the adopted package's .git directory
    let cfg = config::load_config().await?;
    let tracking = resolve_git_tracking(&meta, &cfg);
    if tracking == GitTracking::Workspace {
        let adopted_git_dir = dest.join(".git");
        if adopted_git_dir.exists() {
            tokio::fs::remove_dir_all(&adopted_git_dir)
                .await
                .with_context(|| {
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
    sync_workspace_file(ws_root, &meta).await?;

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
