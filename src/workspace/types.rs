//! Domain types for workspace metadata.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub(super) const WORKSPACE_FILE: &str = "buona.workspace.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceMeta {
    pub(crate) name: String,
}

/// Read workspace metadata from a directory, if a `buona.workspace.json` exists.
///
/// Returns `Ok(None)` when the file is simply absent, and `Err` when the file
/// exists but cannot be read or parsed.
pub(crate) fn read_meta(dir: &Path) -> Result<Option<WorkspaceMeta>> {
    let path = dir.join(WORKSPACE_FILE);
    match fs::read_to_string(&path) {
        Ok(contents) => {
            let meta = serde_json::from_str(&contents)
                .with_context(|| format!("invalid {WORKSPACE_FILE} in {}", dir.display()))?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e)
            .with_context(|| format!("could not read {WORKSPACE_FILE} in {}", dir.display())),
    }
}

/// Write workspace metadata to the given directory.
pub(super) fn write_meta(dir: &Path, meta: &WorkspaceMeta) -> Result<()> {
    let meta_path = dir.join(WORKSPACE_FILE);
    let json = serde_json::to_string_pretty(meta)?;
    fs::write(&meta_path, json + "\n")
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
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-project");
    }

    #[test]
    fn workspace_meta_ignores_legacy_packages_field() {
        // Old metadata files may contain a "packages" array — serde should
        // silently ignore it.
        let json = r#"{"name": "old-workspace", "packages": [{"name": "pkg", "url": "x"}]}"#;
        let meta: WorkspaceMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.name, "old-workspace");
    }

    #[test]
    fn read_meta_returns_some_for_valid_workspace() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test-workspace".to_string(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = read_meta(dir.path()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "test-workspace");
    }

    #[test]
    fn read_meta_returns_none_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = read_meta(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn write_meta_creates_file() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
        };
        write_meta(dir.path(), &meta).unwrap();

        let result = read_meta(dir.path()).unwrap().unwrap();
        assert_eq!(result.name, "test");
    }
}
