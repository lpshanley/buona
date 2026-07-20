//! Domain types for workspace metadata.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{BuonaConfig, GitTracking};

pub(super) const WORKSPACE_FILE: &str = "buona.workspace.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceMeta {
    pub(crate) name: String,

    /// Optional per-workspace git tracking mode override.
    /// When `None`, falls back to the global config default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) git_tracking: Option<GitTracking>,

    /// Whether to include workspace root (`.`) in generated `.code-workspace`
    /// folder entries.
    ///
    /// When `None`, behaves like `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mount_root: Option<bool>,
}

impl WorkspaceMeta {
    /// Resolve the effective git tracking mode for this workspace.
    ///
    /// Priority: workspace-level override > global config default.
    pub(crate) fn effective_tracking(&self, cfg: &BuonaConfig) -> GitTracking {
        self.git_tracking.unwrap_or(cfg.git.tracking)
    }
}

/// Read workspace metadata from a directory, if a `buona.workspace.json` exists.
///
/// Returns `Ok(None)` when the file is simply absent, and `Err` when the file
/// exists but cannot be read or parsed.
pub(crate) async fn read_meta(dir: &Path) -> Result<Option<WorkspaceMeta>> {
    let path = dir.join(WORKSPACE_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let meta = serde_json::from_str(&contents)
                .with_context(|| format!("invalid {WORKSPACE_FILE} in {}", dir.display()))?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => {
            Err(e).with_context(|| format!("could not read {WORKSPACE_FILE} in {}", dir.display()))
        }
    }
}

/// Write workspace metadata to the given directory.
pub(super) async fn write_meta(dir: &Path, meta: &WorkspaceMeta) -> Result<()> {
    let meta_path = dir.join(WORKSPACE_FILE);
    let json = serde_json::to_string_pretty(meta)?;
    crate::fsutil::write_atomic(&meta_path, &(json + "\n"))
        .await
        .with_context(|| format!("could not write {WORKSPACE_FILE}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn workspace_meta_round_trips_through_serde() {
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            git_tracking: None,
            mount_root: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-project");
        assert_eq!(deserialized.git_tracking, None);
        assert_eq!(deserialized.mount_root, None);
    }

    #[test]
    fn workspace_meta_with_git_tracking_round_trips() {
        let meta = WorkspaceMeta {
            name: "mono".to_string(),
            git_tracking: Some(GitTracking::Workspace),
            mount_root: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.git_tracking, Some(GitTracking::Workspace));
    }

    #[test]
    fn workspace_meta_none_tracking_omitted_in_json() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
            mount_root: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("git_tracking"));
        assert!(!json.contains("mount_root"));
    }

    #[test]
    fn workspace_meta_mount_root_round_trips() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
            mount_root: Some(true),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mount_root, Some(true));
    }

    #[tokio::test]
    async fn read_meta_returns_some_for_valid_workspace() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test-workspace".to_string(),
            git_tracking: None,
            mount_root: None,
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        std::fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = read_meta(dir.path()).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "test-workspace");
    }

    #[tokio::test]
    async fn read_meta_returns_none_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = read_meta(dir.path()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn write_meta_creates_file() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
            mount_root: None,
        };
        write_meta(dir.path(), &meta).await.unwrap();

        let result = read_meta(dir.path()).await.unwrap().unwrap();
        assert_eq!(result.name, "test");
    }

    #[test]
    fn workspace_meta_ignores_unknown_fields() {
        let json = r#"{"name":"test","mount_root":true,"legacy_flag":true}"#;
        let meta: WorkspaceMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.name, "test");
        assert_eq!(meta.mount_root, Some(true));

        let written = serde_json::to_string(&meta).unwrap();
        assert!(!written.contains("legacy_flag"));
    }
}
