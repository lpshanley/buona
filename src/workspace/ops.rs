//! Workspace operations — thin orchestration layer.
//!
//! Each public function resolves the workspace target and delegates to a
//! focused module (`add_packages`, `remove_packages`, `sync_packages`,
//! `adopt_package`, `open_workspace`, `info`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::config;
use crate::config::GitTracking;
use crate::styles::Styles;

use super::add_packages::add_packages_to_workspace;
use super::adopt_package::adopt_into_workspace;
use super::git_ops;
use super::info::show_info;
use super::locator;
use super::open_workspace::open_workspace_at;
use super::remove_packages::remove_packages_from_workspace;
use super::sync_packages::sync_workspace;
use super::types::{WorkspaceMeta, read_meta, write_meta};
use super::workspace_file::sync_workspace_file;

/// Resolve workspace root from an optional workspace selector.
///
/// If a selector is provided, lookup by name/directory. Otherwise, detect from
/// current working directory.
async fn resolve_workspace_target(workspace: Option<&str>) -> Result<PathBuf> {
    locator::resolve_workspace_target(workspace).await
}

/// Resolve the effective git tracking mode for a workspace.
///
/// Priority: workspace-level override > global config default > hardcoded Package.
fn resolve_git_tracking(meta: &WorkspaceMeta, cfg: &config::BuonaConfig) -> GitTracking {
    meta.git_tracking.unwrap_or(cfg.git.tracking)
}

/// List all workspaces (directories) found in the configured workspace directory.
pub(crate) async fn list() -> Result<()> {
    let workspace_dir = config::workspace_dir().await?;
    let s = Styles::default();

    println!();
    println!("  {}", s.bold.apply_to("Workspaces"));
    println!("  {}", s.dim.apply_to("──────────"));

    if !workspace_dir.exists() {
        bail!(
            "workspace directory does not exist: {}\n  Run {} to configure it.",
            workspace_dir.display(),
            "buona config setup",
        );
    }

    let mut entries = tokio::fs::read_dir(&workspace_dir).await.with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    let mut workspaces: Vec<(String, WorkspaceMeta)> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            if let Some(meta) = read_meta(&entry.path()).await? {
                workspaces.push((dir_name, meta));
            }
        }
    }

    workspaces.sort_by(|a, b| a.0.cmp(&b.0));

    if workspaces.is_empty() {
        println!(
            "  {}",
            s.dim.apply_to(format!(
                "No workspaces found in {}",
                workspace_dir.display()
            ))
        );
    } else {
        println!(
            "  {}  {}",
            s.dim.apply_to("Directory:"),
            workspace_dir.display()
        );
        println!();
        for (dir_name, meta) in &workspaces {
            if meta.name != *dir_name {
                println!(
                    "  {}  {} {}",
                    s.cyan.apply_to("•"),
                    meta.name,
                    s.dim.apply_to(format!("({dir_name})"))
                );
            } else {
                println!("  {}  {dir_name}", s.cyan.apply_to("•"));
            }
        }
    }

    println!();
    Ok(())
}

/// Create a new workspace directory. Writes a `buona.workspace.json` marker
/// file with the workspace name. If `name` is not provided, the directory name
/// is used.
///
/// After creation, if `packages` is provided, they are cloned into the workspace.
/// If `open_ws` is true, the workspace is opened in the configured editor.
pub(crate) async fn create(
    path: &Path,
    name: Option<&str>,
    packages: Option<&[String]>,
    open_ws: bool,
    git_tracking: Option<GitTracking>,
) -> Result<()> {
    let s = Styles::default();

    // Resolve the target directory
    let target: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        config::workspace_dir().await?.join(path)
    };

    // Derive the workspace name
    let ws_name = match name {
        Some(n) => n.to_string(),
        None => target
            .file_name()
            .context("could not determine directory name from path")?
            .to_string_lossy()
            .into_owned(),
    };

    if target.exists() {
        bail!("directory already exists: {}", target.display());
    }

    // Create the workspace directory (and any parent directories)
    tokio::fs::create_dir_all(&target)
        .await
        .with_context(|| format!("could not create workspace directory: {}", target.display()))?;

    // Create the src/ directory for packages
    tokio::fs::create_dir_all(target.join("src"))
        .await
        .with_context(|| {
            format!(
                "could not create src directory: {}",
                target.join("src").display()
            )
        })?;

    // Write the workspace metadata file
    let meta = WorkspaceMeta {
        name: ws_name,
        git_tracking,
    };
    write_meta(&target, &meta).await?;

    // Initialize git at workspace root if workspace-level tracking is active
    let cfg = config::load_config().await?;
    let tracking = resolve_git_tracking(&meta, &cfg);

    if tracking == GitTracking::Workspace {
        let output = git_ops::init_repo(&target).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git init failed: {}", stderr.trim());
        }

        println!(
            "  {} Initialized git repository at workspace root",
            s.green.apply_to("✔"),
        );
    }

    // Auto-sync the .code-workspace file
    sync_workspace_file(&target, &meta).await?;

    println!();
    println!(
        "  {} Created workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}  {}", s.dim.apply_to("Location:"), target.display());

    // Add packages if specified
    if let Some(pkgs) = packages
        && !pkgs.is_empty()
    {
        add_packages_to_workspace(&target, pkgs).await?;
    }

    // Open workspace in editor if requested
    if open_ws {
        open_workspace_at(&target).await?;
    }

    println!();
    Ok(())
}

/// Delete a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub(crate) async fn delete(query: &str, force: bool) -> Result<()> {
    let s = Styles::default();

    let target = locator::find_workspace(query).await?;

    let meta = read_meta(&target).await?;
    let display_name = meta.as_ref().map(|m| m.name.as_str()).unwrap_or(query);

    if !force {
        println!();
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "  Delete workspace {} at {}?",
                s.bold.apply_to(display_name),
                s.dim.apply_to(target.display().to_string())
            ))
            .default(false)
            .interact()
            .context("failed to read input")?;

        if !confirmed {
            println!("  Aborted.");
            println!();
            return Ok(());
        }
    }

    tokio::fs::remove_dir_all(&target)
        .await
        .with_context(|| format!("could not delete workspace directory: {}", target.display()))?;

    println!();
    println!(
        "  {} Deleted workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(display_name)
    );
    println!();

    Ok(())
}

/// Add one or more packages to a workspace by cloning them into `src/`.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn add(packages: &[String], workspace: Option<&str>) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace).await?;
    add_packages_to_workspace(&ws_root, packages).await
}

/// Remove one or more packages from a workspace.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn remove_packages(
    packages: &[String],
    workspace: Option<&str>,
    force: bool,
) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace).await?;
    remove_packages_from_workspace(&ws_root, packages, force).await
}

/// Pull (or fetch) the latest changes for tracked packages and re-sync the
/// `.code-workspace` file.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn sync(
    packages: &[String],
    workspace: Option<&str>,
    fetch_only: bool,
) -> Result<PathBuf> {
    let ws_root = resolve_workspace_target(workspace).await?;
    sync_workspace(&ws_root, packages, fetch_only).await
}

/// Pretty-print detailed information about a workspace.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn info(workspace: Option<&str>, json: bool) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace).await?;
    show_info(&ws_root, json).await
}

/// Open a workspace in the configured editor.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn open(workspace: Option<&str>) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace).await?;
    open_workspace_at(&ws_root).await
}

/// Adopt an existing local directory into the workspace.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) async fn adopt(
    path: &Path,
    workspace: Option<&str>,
    copy: bool,
    name_override: Option<&str>,
) -> Result<()> {
    let ws_root = resolve_workspace_target(workspace).await?;
    adopt_into_workspace(&ws_root, path, copy, name_override).await
}

#[cfg(test)]
mod tests {
    use super::super::packages::list_package_names;
    use super::*;
    use tempfile::TempDir;

    async fn list_packages(ws_root: &Path) -> Result<Vec<String>> {
        list_package_names(ws_root).await
    }

    async fn detect_git_remote_url(dir: &Path) -> String {
        git_ops::detect_remote_url(dir).await
    }

    async fn detect_git_branch(dir: &Path) -> String {
        git_ops::detect_branch(dir).await
    }

    /// Helper: create a workspace directory with metadata and a `src/` dir.
    async fn setup_workspace(dir: &Path, name: &str) {
        setup_workspace_with_tracking(dir, name, None).await;
    }

    /// Helper: create a workspace with an explicit git tracking mode.
    async fn setup_workspace_with_tracking(dir: &Path, name: &str, tracking: Option<GitTracking>) {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let meta = WorkspaceMeta {
            name: name.to_string(),
            git_tracking: tracking,
        };
        write_meta(dir, &meta).await.unwrap();
    }

    // ── find_workspace_root tests ────────────────────────────────────

    #[tokio::test]
    async fn find_workspace_root_in_workspace_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let result = locator::find_workspace_root(dir.path()).await.unwrap();
        assert_eq!(result, dir.path());
    }

    #[tokio::test]
    async fn find_workspace_root_in_child_dir() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        // Create a child directory and search from there
        let child = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&child).unwrap();

        let result = locator::find_workspace_root(&child).await.unwrap();
        assert_eq!(result, dir.path());
    }

    #[tokio::test]
    async fn find_workspace_root_fails_when_not_in_workspace() {
        let dir = TempDir::new().unwrap();
        let result = locator::find_workspace_root(dir.path()).await;
        assert!(result.is_err());
    }

    // ── list_packages tests ─────────────────────────────────────────

    #[tokio::test]
    async fn list_packages_returns_sorted_directory_names() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("gamma")).unwrap();
        std::fs::create_dir_all(src.join("alpha")).unwrap();
        std::fs::create_dir_all(src.join("beta")).unwrap();

        let names = list_packages(dir.path()).await.unwrap();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[tokio::test]
    async fn list_packages_ignores_files_in_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("real-pkg")).unwrap();
        std::fs::write(src.join("not-a-package.txt"), "hello").unwrap();

        let names = list_packages(dir.path()).await.unwrap();
        assert_eq!(names, vec!["real-pkg"]);
    }

    #[tokio::test]
    async fn list_packages_returns_empty_when_no_src() {
        let dir = TempDir::new().unwrap();
        // No src/ directory at all
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        write_meta(dir.path(), &meta).await.unwrap();

        let names = list_packages(dir.path()).await.unwrap();
        assert!(names.is_empty());
    }

    #[tokio::test]
    async fn list_packages_returns_empty_when_src_is_empty() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let names = list_packages(dir.path()).await.unwrap();
        assert!(names.is_empty());
    }

    // ── detect_git_remote_url tests ─────────────────────────────────

    #[tokio::test]
    async fn detect_git_remote_url_finds_origin() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("git-repo");
        std::fs::create_dir_all(&repo).unwrap();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/test.git"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let url = detect_git_remote_url(&repo).await;
        assert_eq!(url, "git@github.com:acme/test.git");
    }

    #[tokio::test]
    async fn detect_git_remote_url_returns_empty_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        let plain = dir.path().join("plain-dir");
        std::fs::create_dir_all(&plain).unwrap();

        let url = detect_git_remote_url(&plain).await;
        assert!(url.is_empty());
    }

    // ── detect_git_branch tests ─────────────────────────────────────

    #[tokio::test]
    async fn detect_git_branch_finds_current_branch() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("git-repo");
        std::fs::create_dir_all(&repo).unwrap();

        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        // HEAD requires at least one commit to resolve
        std::fs::write(repo.join("README.md"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@test.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(&repo)
            .output()
            .unwrap();

        let branch = detect_git_branch(&repo).await;
        assert_eq!(branch, "main");
    }

    #[tokio::test]
    async fn detect_git_branch_returns_empty_for_non_git_dir() {
        let dir = TempDir::new().unwrap();
        let plain = dir.path().join("plain-dir");
        std::fs::create_dir_all(&plain).unwrap();

        let branch = detect_git_branch(&plain).await;
        assert!(branch.is_empty());
    }

    // ── remove_packages tests ───────────────────────────────────────

    #[tokio::test]
    async fn remove_deletes_directory_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("alpha")).unwrap();
        std::fs::create_dir_all(src.join("beta")).unwrap();
        std::fs::create_dir_all(src.join("gamma")).unwrap();

        // Simulate removing "beta": delete its directory
        let pkg_dir = src.join("beta");
        std::fs::remove_dir_all(&pkg_dir).unwrap();

        assert!(!pkg_dir.exists());
        let remaining = list_packages(dir.path()).await.unwrap();
        assert_eq!(remaining, vec!["alpha", "gamma"]);
    }

    #[tokio::test]
    async fn remove_multiple_directories_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "test").await;

        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("alpha")).unwrap();
        std::fs::create_dir_all(src.join("beta")).unwrap();
        std::fs::create_dir_all(src.join("gamma")).unwrap();

        // Remove "alpha" and "gamma"
        std::fs::remove_dir_all(src.join("alpha")).unwrap();
        std::fs::remove_dir_all(src.join("gamma")).unwrap();

        let remaining = list_packages(dir.path()).await.unwrap();
        assert_eq!(remaining, vec!["beta"]);
    }

    // ── sync_workspace_file tests ───────────────────────────────────

    #[tokio::test]
    async fn sync_workspace_file_derives_folders_from_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "my-project").await;

        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("toolkit")).unwrap();
        std::fs::create_dir_all(src.join("utils")).unwrap();

        let meta = read_meta(dir.path()).await.unwrap().unwrap();
        let ws_file_path = sync_workspace_file(dir.path(), &meta).await.unwrap();

        assert!(ws_file_path.exists());

        let contents = std::fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/toolkit");
        assert_eq!(parsed["folders"][0]["name"], "toolkit");
        assert_eq!(parsed["folders"][1]["path"], "src/utils");
        assert_eq!(parsed["folders"][1]["name"], "utils");
    }

    #[tokio::test]
    async fn sync_workspace_file_empty_src() {
        let dir = TempDir::new().unwrap();
        setup_workspace(dir.path(), "empty-ws").await;

        let meta = read_meta(dir.path()).await.unwrap().unwrap();
        let ws_file_path = sync_workspace_file(dir.path(), &meta).await.unwrap();

        let contents = std::fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"].as_array().unwrap().len(), 0);
    }

    // ── adopt tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn adopt_moves_directory_into_src() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        // Create a source directory outside the workspace
        let outside = TempDir::new().unwrap();
        let source = outside.path().join("my-pkg");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("hello.txt"), "world").unwrap();

        let source_canonical = source.canonicalize().unwrap();

        let src_dir = ws_dir.path().join("src");
        let dest = src_dir.join("my-pkg");
        assert!(!dest.exists());

        // Simulate the adopt logic: move source into ws/src/my-pkg
        if std::fs::rename(&source_canonical, &dest).is_err() {
            std::process::Command::new("cp")
                .args(["-a"])
                .arg(&source_canonical)
                .arg(&dest)
                .status()
                .unwrap();
            std::fs::remove_dir_all(&source_canonical).unwrap();
        }

        // Verify the directory moved
        assert!(dest.join("hello.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("hello.txt")).unwrap(),
            "world"
        );
        assert!(!source.exists(), "original should be gone after move");

        // Package is discovered from filesystem
        let pkgs = list_packages(ws_dir.path()).await.unwrap();
        assert_eq!(pkgs, vec!["my-pkg"]);
    }

    #[tokio::test]
    async fn adopt_copies_when_flag_set() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        let outside = TempDir::new().unwrap();
        let source = outside.path().join("copy-pkg");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("data.txt"), "keep me").unwrap();

        let dest = ws_dir.path().join("src").join("copy-pkg");

        // Copy instead of move
        let status = std::process::Command::new("cp")
            .args(["-a"])
            .arg(&source)
            .arg(&dest)
            .status()
            .unwrap();
        assert!(status.success());

        // Verify destination has the file
        assert!(dest.join("data.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("data.txt")).unwrap(),
            "keep me"
        );
        // Original still exists
        assert!(source.exists(), "original should still exist after copy");

        // Package is discovered from filesystem
        let pkgs = list_packages(ws_dir.path()).await.unwrap();
        assert_eq!(pkgs, vec!["copy-pkg"]);
    }

    #[tokio::test]
    async fn adopt_registers_already_in_src() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        // Place a directory directly in src/
        let pkg_dir = ws_dir.path().join("src").join("existing-pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("marker.txt"), "I was here").unwrap();

        // The package is already discoverable — no move/copy needed
        let pkgs = list_packages(ws_dir.path()).await.unwrap();
        assert_eq!(pkgs, vec!["existing-pkg"]);

        // File is still intact
        assert_eq!(
            std::fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "I was here"
        );
    }

    #[tokio::test]
    async fn adopt_detects_existing_directory_as_conflict() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        // Place a directory in src/ with the name "taken"
        std::fs::create_dir_all(ws_dir.path().join("src").join("taken")).unwrap();

        // The package already exists on disk — adopt should detect the conflict
        let pkgs = list_packages(ws_dir.path()).await.unwrap();
        assert!(pkgs.contains(&"taken".to_string()));
    }

    #[test]
    fn adopt_errors_on_nonexistent_path() {
        let dir = TempDir::new().unwrap();
        let bogus = dir.path().join("does-not-exist");
        assert!(!bogus.exists());
    }

    #[test]
    fn adopt_errors_on_file_path() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not-a-dir.txt");
        std::fs::write(&file, "hello").unwrap();
        assert!(file.exists());
        assert!(!file.is_dir(), "files should be rejected by adopt");
    }

    // ── add_packages_to_workspace tests ──────────────────────────────

    #[tokio::test]
    async fn add_packages_to_workspace_clones_git_repo() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        // Create a mock "remote" git repo
        let remote_dir = TempDir::new().unwrap();
        let remote_repo = remote_dir.path().join("test-pkg");
        std::fs::create_dir_all(&remote_repo).unwrap();
        std::fs::write(remote_repo.join("README.md"), "test").unwrap();

        // Init git repo and commit the file
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&remote_repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();

        // Clone using full file:// URL
        let url = format!("file://{}", remote_repo.canonicalize().unwrap().display());
        let result = add_packages_to_workspace(ws_dir.path(), &[url]).await;

        // The clone should succeed
        assert!(result.is_ok());

        // Verify the package was cloned into src/
        let pkg_dir = ws_dir.path().join("src").join("test-pkg");
        assert!(pkg_dir.exists());
        assert!(pkg_dir.join("README.md").exists());

        // Verify it's discoverable
        let pkgs = list_packages(ws_dir.path()).await.unwrap();
        assert!(pkgs.contains(&"test-pkg".to_string()));
    }

    #[tokio::test]
    async fn add_packages_to_workspace_errors_when_all_fail() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "test-ws").await;

        // Pre-create a package directory
        let pkg_dir = ws_dir.path().join("src").join("existing-pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("marker.txt"), "original").unwrap();

        // Try to add the same package - should fail with "already exists"
        let remote_dir = TempDir::new().unwrap();
        let remote_repo = remote_dir.path().join("existing-pkg");
        std::fs::create_dir_all(&remote_repo).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&remote_repo)
            .output()
            .unwrap();

        let url = format!("file://{}", remote_repo.canonicalize().unwrap().display());
        let result = add_packages_to_workspace(ws_dir.path(), &[url]).await;

        // Should return an error when all packages fail
        assert!(result.is_err());

        // Original file should still be there
        assert_eq!(
            std::fs::read_to_string(pkg_dir.join("marker.txt")).unwrap(),
            "original"
        );
    }

    // ── workspace file generation tests ────────────────────────────

    #[tokio::test]
    async fn sync_workspace_file_generates_workspace_file() {
        let ws_dir = TempDir::new().unwrap();
        setup_workspace(ws_dir.path(), "my-workspace").await;

        // Add a package so there's something in the workspace file
        let src_dir = ws_dir.path().join("src");
        std::fs::create_dir_all(src_dir.join("pkg-a")).unwrap();

        // Call sync_workspace_file - this should create the .code-workspace file.
        let result = sync_workspace_file(
            ws_dir.path(),
            &read_meta(ws_dir.path()).await.unwrap().unwrap(),
        )
        .await;
        assert!(result.is_ok());

        let ws_file = result.unwrap();
        assert!(ws_file.exists());
        assert!(
            ws_file
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("my-workspace")
        );

        // Verify file contents
        let contents = std::fs::read_to_string(&ws_file).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/pkg-a");
    }

    // ── resolve_git_tracking tests ───────────────────────────────

    #[test]
    fn resolve_tracking_workspace_override_wins() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: Some(GitTracking::Workspace),
        };
        let cfg = config::BuonaConfig::default(); // default tracking = Package
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Workspace);
    }

    #[test]
    fn resolve_tracking_falls_back_to_global() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        let mut cfg = config::BuonaConfig::default();
        cfg.git.tracking = GitTracking::Workspace;
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Workspace);
    }

    #[test]
    fn resolve_tracking_defaults_to_package() {
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            git_tracking: None,
        };
        let cfg = config::BuonaConfig::default();
        assert_eq!(resolve_git_tracking(&meta, &cfg), GitTracking::Package);
    }
}
