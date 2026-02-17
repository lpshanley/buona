//! Target resolution for `buona run` and `buona detect`.

use std::path::{Path, PathBuf};

use crate::workspace;

use super::error::RunError;

#[derive(Debug, Clone)]
pub(super) struct ExecutionTarget {
    pub(super) name: String,
    pub(super) dir: PathBuf,
    pub(super) is_workspace_root: bool,
}

impl ExecutionTarget {
    pub(super) fn label(&self) -> String {
        if self.is_workspace_root {
            "root".to_string()
        } else {
            self.name.clone()
        }
    }
}

pub(super) async fn resolve_targets(
    cwd: &Path,
    ws_root: &Path,
    target_names: &[String],
    recursive: bool,
) -> Result<Vec<ExecutionTarget>, RunError> {
    if recursive {
        let mut targets = vec![ExecutionTarget {
            name: "root".to_string(),
            dir: ws_root.to_path_buf(),
            is_workspace_root: true,
        }];
        targets.extend(list_workspace_package_targets(ws_root).await?);
        return Ok(targets);
    }

    if target_names.is_empty() {
        return Ok(vec![resolve_closest_target(cwd, ws_root)?]);
    }

    let mut targets = Vec::new();
    for target_name in target_names {
        if target_name == "root" {
            targets.push(ExecutionTarget {
                name: "root".to_string(),
                dir: ws_root.to_path_buf(),
                is_workspace_root: true,
            });
            continue;
        }

        let pkg_dir = ws_root.join("src").join(target_name);
        if !pkg_dir.is_dir() {
            return Err(RunError::ConfigError(format!(
                "unknown target \"{target_name}\" in workspace {}",
                ws_root.display()
            )));
        }

        targets.push(ExecutionTarget {
            name: target_name.clone(),
            dir: pkg_dir,
            is_workspace_root: false,
        });
    }

    Ok(targets)
}

fn resolve_closest_target(cwd: &Path, ws_root: &Path) -> Result<ExecutionTarget, RunError> {
    if let Ok(pkg_dir) = resolve_package_dir(cwd, ws_root) {
        let pkg_name = pkg_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        return Ok(ExecutionTarget {
            name: pkg_name,
            dir: pkg_dir,
            is_workspace_root: false,
        });
    }

    Ok(ExecutionTarget {
        name: "root".to_string(),
        dir: ws_root.to_path_buf(),
        is_workspace_root: true,
    })
}

/// Determine the package root directory from the current working directory.
///
/// Finds which `src/<name>/` directory the cwd is inside. Returns the package
/// directory (e.g., `ws_root/src/my-pkg`).
pub(super) fn resolve_package_dir(cwd: &Path, ws_root: &Path) -> Result<PathBuf, RunError> {
    let src_dir = ws_root.join("src");

    if let Ok(relative) = cwd.strip_prefix(&src_dir)
        && let Some(pkg_component) = relative.components().next()
    {
        let pkg_name = pkg_component.as_os_str().to_string_lossy();
        let pkg_dir = src_dir.join(pkg_name.as_ref());
        if pkg_dir.is_dir() {
            return Ok(pkg_dir);
        }
    }

    Err(RunError::NoPackageResolved(
        "could not determine which package you are in.\n  \
         Run this command from within a package directory (under src/)."
            .to_string(),
    ))
}

pub(super) async fn list_workspace_package_targets(
    ws_root: &Path,
) -> Result<Vec<ExecutionTarget>, RunError> {
    let names = workspace::list_package_names(ws_root)
        .await
        .map_err(|e| RunError::ConfigError(format!("{e}")))?;

    Ok(names
        .into_iter()
        .map(|name| ExecutionTarget {
            dir: ws_root.join("src").join(&name),
            name,
            is_workspace_root: false,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_workspace_with_package(ws_dir: &Path, ws_name: &str, pkg_name: &str) -> PathBuf {
        fs::create_dir_all(ws_dir.join("src").join(pkg_name)).unwrap();
        let json = serde_json::json!({ "name": ws_name });
        fs::write(
            ws_dir.join("buona.workspace.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
        ws_dir.join("src").join(pkg_name)
    }

    #[test]
    fn resolve_from_package_root() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let result = resolve_package_dir(&pkg_dir, dir.path()).unwrap();
        assert_eq!(result, pkg_dir);
    }

    #[test]
    fn resolve_from_deep_inside_package() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let deep = pkg_dir.join("src").join("nested");
        fs::create_dir_all(&deep).unwrap();

        let result = resolve_package_dir(&deep, dir.path()).unwrap();
        assert_eq!(result, pkg_dir);
    }

    #[test]
    fn resolve_fails_at_workspace_root() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let result = resolve_package_dir(dir.path(), dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn resolve_fails_at_src_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "my-pkg");

        let src = dir.path().join("src");
        let result = resolve_package_dir(&src, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn resolve_with_multiple_packages() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");
        fs::create_dir_all(dir.path().join("src").join("pkg-b")).unwrap();

        let result_a =
            resolve_package_dir(&dir.path().join("src").join("pkg-a"), dir.path()).unwrap();
        let result_b =
            resolve_package_dir(&dir.path().join("src").join("pkg-b"), dir.path()).unwrap();

        assert_eq!(result_a.file_name().unwrap().to_string_lossy(), "pkg-a");
        assert_eq!(result_b.file_name().unwrap().to_string_lossy(), "pkg-b");
    }

    #[tokio::test]
    async fn resolve_targets_closest_defaults_to_package() {
        let dir = TempDir::new().unwrap();
        let pkg_dir = setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");

        let targets = resolve_targets(&pkg_dir, dir.path(), &[], false)
            .await
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].label(), "pkg-a");
    }

    #[tokio::test]
    async fn resolve_targets_closest_defaults_to_root() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");

        let targets = resolve_targets(dir.path(), dir.path(), &[], false)
            .await
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].label(), "root");
    }

    #[tokio::test]
    async fn resolve_targets_respects_ordered_explicit_targets() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "pkg-a");
        fs::create_dir_all(dir.path().join("src").join("pkg-b")).unwrap();

        let requested = vec!["pkg-b".to_string(), "root".to_string(), "pkg-a".to_string()];
        let targets = resolve_targets(dir.path(), dir.path(), &requested, false)
            .await
            .unwrap();
        let labels: Vec<String> = targets.iter().map(|t| t.label()).collect();
        assert_eq!(labels, vec!["pkg-b", "root", "pkg-a"]);
    }

    #[tokio::test]
    async fn resolve_targets_recursive_includes_root_and_sorted_packages() {
        let dir = TempDir::new().unwrap();
        setup_workspace_with_package(dir.path(), "test-ws", "zeta");
        fs::create_dir_all(dir.path().join("src").join("alpha")).unwrap();

        let targets = resolve_targets(dir.path(), dir.path(), &[], true)
            .await
            .unwrap();
        let labels: Vec<String> = targets.iter().map(|t| t.label()).collect();
        assert_eq!(labels, vec!["root", "alpha", "zeta"]);
    }
}
