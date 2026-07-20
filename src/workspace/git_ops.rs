//! Git process helpers for workspace operations.

use std::path::Path;
use std::process::Output;

use tokio::process::Command;

use anyhow::{Context, Result};

/// Detect the git remote origin URL for a directory, if it is a git repo.
///
/// Returns an empty string when the directory is not a git repo, has no origin,
/// or the command fails.
pub(super) async fn detect_remote_url(dir: &Path) -> String {
    let result = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .await;
    result
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if url.is_empty() { None } else { Some(url) }
        })
        .unwrap_or_default()
}

/// Detect the current git branch for a directory.
///
/// Returns an empty string when the directory is not a git repo, HEAD is
/// detached, or the command fails.
pub(super) async fn detect_branch(dir: &Path) -> String {
    let result = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await;
    result
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if branch.is_empty() || branch == "HEAD" {
                None
            } else {
                Some(branch)
            }
        })
        .unwrap_or_default()
}

/// Run `git init` in the target directory.
pub(super) async fn init_repo(dir: &Path) -> Result<Output> {
    Command::new("git")
        .arg("init")
        .current_dir(dir)
        .output()
        .await
        .context("failed to execute git init — is git installed?")
}

/// Run `git clone <url> <dest>`.
pub(super) async fn clone_into(url: &str, dest: &Path) -> Result<Output> {
    Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(dest)
        .output()
        .await
        .context("failed to execute git clone — is git installed?")
}

/// Run `git pull` or `git fetch` in a repo directory.
pub(super) async fn sync_repo(dir: &Path, fetch_only: bool) -> Result<Output> {
    let git_arg = if fetch_only { "fetch" } else { "pull" };
    Command::new("git")
        .arg(git_arg)
        .current_dir(dir)
        .output()
        .await
        .with_context(|| format!("failed to execute git {git_arg} — is git installed?"))
}

/// Build a short status summary from git stdout.
pub(super) fn summarize_sync_stdout(output: &Output, fetch_only: bool) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let fallback = if fetch_only { "done" } else { "up to date" };
    let summary = stdout.lines().next().unwrap_or(fallback).trim();
    if summary.is_empty() {
        fallback.to_string()
    } else {
        summary.to_string()
    }
}
