//! Domain types for workspace metadata and package entries.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub(super) const WORKSPACE_FILE: &str = "buona.workspace.json";

/// A tracked package that has been added to a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PackageEntry {
    pub(crate) name: String,
    pub(crate) url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceMeta {
    pub(crate) name: String,

    /// Packages added to this workspace via `buona ws add`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) packages: Vec<PackageEntry>,
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
            packages: Vec::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-project");
        assert!(deserialized.packages.is_empty());
    }

    #[test]
    fn workspace_meta_with_packages_round_trips() {
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            packages: vec![PackageEntry {
                name: "toolkit".to_string(),
                url: "git@github.com:acme/toolkit.git".to_string(),
            }],
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.packages.len(), 1);
        assert_eq!(deserialized.packages[0].name, "toolkit");
        assert_eq!(
            deserialized.packages[0].url,
            "git@github.com:acme/toolkit.git"
        );
    }

    #[test]
    fn workspace_meta_without_packages_field_defaults_to_empty() {
        let json = r#"{"name": "old-workspace"}"#;
        let meta: WorkspaceMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.name, "old-workspace");
        assert!(meta.packages.is_empty());
    }

    #[test]
    fn workspace_meta_empty_packages_not_serialized() {
        let meta = WorkspaceMeta {
            name: "clean".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("packages"));
    }

    #[test]
    fn read_meta_returns_some_for_valid_workspace() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test-workspace".to_string(),
            packages: Vec::new(),
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
            packages: vec![PackageEntry {
                name: "pkg".to_string(),
                url: "git@github.com:org/pkg.git".to_string(),
            }],
        };
        write_meta(dir.path(), &meta).unwrap();

        let result = read_meta(dir.path()).unwrap().unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, "pkg");
    }
}
