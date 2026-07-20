//! Workspace lookup helpers.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config;

use super::types::{WORKSPACE_FILE, read_meta};

/// Find a workspace by name or directory name. Returns the resolved path.
pub(super) async fn find_workspace(query: &str) -> Result<PathBuf> {
    let workspace_dir = config::workspace_dir().await?;

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct).await?.is_some() {
        return Ok(direct);
    }

    // Otherwise, search by workspace name in metadata
    let mut entries = tokio::fs::read_dir(&workspace_dir).await.with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path).await?
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
pub(crate) async fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let marker = dir.join(WORKSPACE_FILE);
        match tokio::fs::try_exists(&marker).await {
            Ok(true) => return Ok(dir),
            Ok(false) => {}
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "could not check for {} in {}",
                        WORKSPACE_FILE,
                        dir.display()
                    )
                });
            }
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

/// Resolve workspace root from an optional workspace selector.
///
/// If a selector is provided, lookup by name/directory. Otherwise, detect from
/// current working directory.
pub(super) async fn resolve_workspace_target(workspace: Option<&str>) -> Result<PathBuf> {
    match workspace {
        Some(name) => find_workspace(name).await,
        None => {
            let cwd = env::current_dir().context("could not determine current directory")?;
            find_workspace_root(&cwd).await
        }
    }
}
