//! `.code-workspace` file generation.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::packages::list_package_names;
use super::types::WorkspaceMeta;
use super::vscode::{VscodeWorkspaceFolder, sanitize_name};

/// Generate a `.code-workspace` file from the workspace root.
///
/// Derives the folder list by scanning `src/` for subdirectories. Uses
/// `meta.name` to produce the workspace filename. Only the `folders` entry is
/// regenerated — `settings` and any other keys the user (or their editor) has
/// added to the file are preserved. Returns the path to the generated file.
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

    // Preserve everything except `folders` from an existing file, so user
    // settings/extensions/launch config in the .code-workspace survive
    // regeneration.
    let mut doc = read_existing_doc(&ws_file_path)
        .await
        .unwrap_or_else(|| serde_json::json!({ "settings": {} }));
    doc["folders"] = serde_json::to_value(&folders)?;

    let json = serde_json::to_string_pretty(&doc)?;
    crate::fsutil::write_atomic(&ws_file_path, &(json + "\n"))
        .await
        .with_context(|| format!("could not write workspace file: {}", ws_file_path.display()))?;

    Ok(ws_file_path)
}

/// Read the existing `.code-workspace` document, if present and valid.
async fn read_existing_doc(path: &Path) -> Option<Value> {
    let contents = tokio::fs::read_to_string(path).await.ok()?;
    match serde_json::from_str::<Value>(&contents) {
        Ok(value @ Value::Object(_)) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn meta(name: &str) -> WorkspaceMeta {
        WorkspaceMeta {
            name: name.to_string(),
            git_tracking: None,
            mount_root: None,
        }
    }

    #[tokio::test]
    async fn sync_preserves_user_settings() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/pkg-a")).unwrap();

        // Simulate a user having customized settings in the workspace file
        let ws_file = dir.path().join("my-ws.code-workspace");
        std::fs::write(
            &ws_file,
            r#"{
                "folders": [],
                "settings": { "editor.formatOnSave": true },
                "extensions": { "recommendations": ["rust-lang.rust-analyzer"] }
            }"#,
        )
        .unwrap();

        let path = sync_workspace_file(dir.path(), &meta("my-ws"))
            .await
            .unwrap();
        assert_eq!(path, ws_file);

        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["settings"]["editor.formatOnSave"], Value::Bool(true));
        assert_eq!(
            doc["extensions"]["recommendations"][0],
            "rust-lang.rust-analyzer"
        );
        assert_eq!(doc["folders"][0]["path"], "src/pkg-a");
    }

    #[tokio::test]
    async fn sync_creates_default_doc_when_missing() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();

        let path = sync_workspace_file(dir.path(), &meta("fresh"))
            .await
            .unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["settings"], serde_json::json!({}));
        assert!(doc["folders"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sync_recovers_from_corrupt_existing_file() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/pkg")).unwrap();
        std::fs::write(dir.path().join("ws.code-workspace"), "not json {{{").unwrap();

        let path = sync_workspace_file(dir.path(), &meta("ws")).await.unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["folders"][0]["path"], "src/pkg");
    }
}
