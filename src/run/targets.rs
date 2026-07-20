//! Target resolution for `buona run` and `buona detect`.

use std::path::{Path, PathBuf};

use crate::workspace;

use super::error::RunError;

/// Marker file that opts a directory into standalone package-root resolution.
const PACKAGE_CONFIG_FILE: &str = "buona.json";

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

/// Build a single-target execution context for a directory outside a workspace.
///
/// The target directory is `dir` itself; the label is the directory basename.
pub(super) fn local_target(dir: &Path) -> ExecutionTarget {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| ".".to_string());

    ExecutionTarget {
        name,
        dir: dir.to_path_buf(),
        is_workspace_root: false,
    }
}

/// Resolve the standalone execution target for `cwd`.
///
/// Walks up from `cwd` looking for `buona.json`. The first ancestor that
/// contains one becomes the package root. If none is found, falls back to
/// `cwd` itself (marker-only detection, no parent scan).
pub(super) async fn resolve_local_target(cwd: &Path) -> Result<ExecutionTarget, RunError> {
    let root = find_standalone_package_root(cwd).await?;
    Ok(local_target(&root))
}

/// Walk up from `start` for a `buona.json`. Returns that directory, or `start`
/// when no ancestor has the file.
async fn find_standalone_package_root(start: &Path) -> Result<PathBuf, RunError> {
    let mut dir = start.to_path_buf();
    loop {
        let marker = dir.join(PACKAGE_CONFIG_FILE);
        match tokio::fs::try_exists(&marker).await {
            Ok(true) => return Ok(dir),
            Ok(false) => {}
            Err(e) => {
                return Err(RunError::ConfigError(format!(
                    "could not check for {PACKAGE_CONFIG_FILE} in {}: {e}",
                    dir.display()
                )));
            }
        }

        if !dir.pop() {
            return Ok(start.to_path_buf());
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
    fn local_target_uses_cwd_basename() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join("my-app");
        fs::create_dir_all(&project).unwrap();

        let target = local_target(&project);
        assert_eq!(target.name, "my-app");
        assert_eq!(target.dir, project);
        assert!(!target.is_workspace_root);
        assert_eq!(target.label(), "my-app");
    }

    #[tokio::test]
    async fn resolve_local_target_falls_back_to_cwd_without_buona_json() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("src").join("components");
        fs::create_dir_all(&nested).unwrap();

        let target = resolve_local_target(&nested).await.unwrap();
        assert_eq!(target.dir, nested);
        assert_eq!(target.label(), "components");
    }

    #[tokio::test]
    async fn resolve_local_target_walks_up_to_buona_json() {
        let dir = TempDir::new().unwrap();
        let project = dir.path().join("my-app");
        let nested = project.join("src").join("components");
        fs::create_dir_all(&nested).unwrap();
        fs::write(project.join("buona.json"), "{}\n").unwrap();
        fs::write(project.join("package.json"), "{}\n").unwrap();

        let target = resolve_local_target(&nested).await.unwrap();
        assert_eq!(target.dir, project);
        assert_eq!(target.label(), "my-app");
    }

    #[tokio::test]
    async fn resolve_local_target_uses_nearest_buona_json() {
        let dir = TempDir::new().unwrap();
        let outer = dir.path().join("monorepo");
        let inner = outer.join("packages").join("web");
        let nested = inner.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(outer.join("buona.json"), "{\"system\":\"npm\"}\n").unwrap();
        fs::write(inner.join("buona.json"), "{\"system\":\"pnpm\"}\n").unwrap();

        let target = resolve_local_target(&nested).await.unwrap();
        assert_eq!(target.dir, inner);
        assert_eq!(target.label(), "web");
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
