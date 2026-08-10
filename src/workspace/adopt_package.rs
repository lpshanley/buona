//! Adopt an existing local directory into a workspace.

use std::env;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{BuonaConfig, GitTracking};
use crate::fsutil;
use crate::styles::Styles;

use super::types::read_meta;
use super::workspace_file::sync_workspace_file;

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
    cfg: &BuonaConfig,
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

    if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
        bail!("path does not exist: {}", source.display());
    }
    let source_meta = tokio::fs::metadata(&source)
        .await
        .with_context(|| format!("could not read path: {}", source.display()))?;
    if !source_meta.is_dir() {
        bail!(
            "path is not a directory: {}\n  The adopt command requires a directory path.",
            source.display()
        );
    }

    let source = tokio::fs::canonicalize(&source)
        .await
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
    let already_in_place = tokio::fs::try_exists(&dest).await.unwrap_or(false)
        && tokio::fs::canonicalize(&dest)
            .await
            .ok()
            .map(|d| d == source)
            .unwrap_or(false);

    if already_in_place {
        crate::textln!();
        crate::textln!(
            "  {} Directory already at {}",
            s.dim.apply_to("→"),
            s.dim.apply_to(dest.display().to_string())
        );
    } else {
        // Ensure src/ exists
        tokio::fs::create_dir_all(&src_dir)
            .await
            .with_context(|| format!("could not create src directory: {}", src_dir.display()))?;

        if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
            bail!(
                "destination already exists: {}\n  \
                 A directory with the name \"{}\" is already in src/. \
                 Use --name to specify a different name.",
                dest.display(),
                pkg_name
            );
        }

        if copy {
            crate::textln!(
                "  {} Copying {} to {} ...",
                s.dim.apply_to("→"),
                s.cyan.apply_to(&pkg_name),
                s.dim.apply_to(dest.display().to_string())
            );

            copy_dir_into(&source, &dest).await?;
        } else {
            crate::textln!(
                "  {} Moving {} to {} ...",
                s.dim.apply_to("→"),
                s.cyan.apply_to(&pkg_name),
                s.dim.apply_to(dest.display().to_string())
            );

            // Try tokio::fs::rename first (fast, same-filesystem only)
            if tokio::fs::rename(&source, &dest).await.is_err() {
                // Fall back to copy + delete for cross-device moves
                copy_dir_into(&source, &dest).await?;

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
    if meta.effective_tracking(cfg) == GitTracking::Workspace {
        let adopted_git_dir = dest.join(".git");
        if tokio::fs::try_exists(&adopted_git_dir)
            .await
            .unwrap_or(false)
        {
            tokio::fs::remove_dir_all(&adopted_git_dir)
                .await
                .with_context(|| {
                    format!(
                        "could not remove .git directory from adopted package: {}",
                        adopted_git_dir.display()
                    )
                })?;
            crate::textln!(
                "  {} Removed package-level .git (workspace-level tracking active)",
                s.dim.apply_to("→"),
            );
        }
    }

    // Sync the .code-workspace file (picks up the new directory in src/)
    sync_workspace_file(ws_root, &meta).await?;

    crate::textln!();
    crate::textln!(
        "  {} Adopted {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&pkg_name)
    );
    crate::textln!("  {}  {}", s.dim.apply_to("Location:"), dest.display());
    crate::textln!();

    Ok(())
}

/// Copy a directory into a fresh destination, cleaning up on failure so a
/// half-copied package never lingers in `src/`.
async fn copy_dir_into(source: &Path, dest: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dest)
        .await
        .with_context(|| format!("could not create directory: {}", dest.display()))?;

    if let Err(e) = fsutil::copy_dir_recursive(source, dest, |_| false).await {
        let _ = tokio::fs::remove_dir_all(dest).await;
        return Err(e);
    }

    Ok(())
}
