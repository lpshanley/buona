//! `.code-workspace` file generation.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::packages::list_package_names;
use super::types::WorkspaceMeta;
use super::vscode::{VscodeWorkspace, VscodeWorkspaceFolder, sanitize_name};

/// Generate a `.code-workspace` file from the workspace root.
///
/// Derives the folder list by scanning `src/` for subdirectories. Uses
/// `meta.name` to produce the workspace filename. Returns the path to the
/// generated file.
pub(super) async fn sync_workspace_file(ws_root: &Path, meta: &WorkspaceMeta) -> Result<PathBuf> {
    let sanitized = sanitize_name(&meta.name);
    if sanitized.is_empty() {
        bail!(
            "workspace name \"{}\" produces an empty filename after sanitization",
            meta.name
        );
    }

    let filename = format!("{sanitized}.code-workspace");
    let ws_file_path = ws_root.join(&filename);

    let pkg_names = list_package_names(ws_root).await?;
    let mut folders: Vec<VscodeWorkspaceFolder> = pkg_names
        .iter()
        .map(|name| VscodeWorkspaceFolder {
            path: format!("src/{name}"),
            name: name.clone(),
        })
        .collect();

    if meta.mount_root.unwrap_or(false) {
        folders.insert(
            0,
            VscodeWorkspaceFolder {
                path: ".".to_string(),
                name: format!("{}-root", meta.name),
            },
        );
    }

    let vscode_ws = VscodeWorkspace {
        folders,
        settings: serde_json::json!({}),
    };

    let json = serde_json::to_string_pretty(&vscode_ws)?;
    tokio::fs::write(&ws_file_path, json + "\n")
        .await
        .with_context(|| format!("could not write workspace file: {}", ws_file_path.display()))?;

    Ok(ws_file_path)
}
