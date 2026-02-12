use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::styles::Styles;

const WORKSPACE_FILE: &str = "buona.workspace.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub name: String,
}

/// Read workspace metadata from a directory, if a `buona.workspace.json` exists.
pub fn read_meta(dir: &Path) -> Option<WorkspaceMeta> {
    let path = dir.join(WORKSPACE_FILE);
    let contents = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Find a workspace by name or directory name. Returns the resolved path.
fn find_workspace(query: &str) -> Result<PathBuf> {
    let workspace_dir = config::workspace_dir()?;

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct).is_some() {
        return Ok(direct);
    }

    // Otherwise, search by workspace name in metadata
    let entries = fs::read_dir(&workspace_dir)
        .with_context(|| {
            format!(
                "could not read workspace directory: {}",
                workspace_dir.display()
            )
        })?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path) {
                if meta.name == query {
                    return Ok(path);
                }
            }
        }
    }

    bail!("no workspace found matching \"{query}\"")
}

/// List all workspaces (directories) found in the configured workspace directory.
pub fn list() -> Result<()> {
    let workspace_dir = config::workspace_dir()?;
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

    let entries = fs::read_dir(&workspace_dir).with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    let mut workspaces: Vec<(String, WorkspaceMeta)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                let meta = read_meta(&entry.path())?;
                Some((dir_name, meta))
            } else {
                None
            }
        })
        .collect();

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
pub fn create(path: &str, name: Option<&str>) -> Result<()> {
    let s = Styles::default();

    // Resolve the target directory
    let target: PathBuf = if PathBuf::from(path).is_absolute() {
        PathBuf::from(path)
    } else {
        config::workspace_dir()?.join(path)
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
    fs::create_dir_all(&target).with_context(|| {
        format!(
            "could not create workspace directory: {}",
            target.display()
        )
    })?;

    // Write the workspace metadata file
    let meta = WorkspaceMeta { name: ws_name };
    let meta_path = target.join(WORKSPACE_FILE);
    let json = serde_json::to_string_pretty(&meta)?;
    fs::write(&meta_path, json + "\n")
        .with_context(|| format!("could not write {WORKSPACE_FILE}"))?;

    println!();
    println!(
        "  {} Created workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&meta.name)
    );
    println!(
        "  {}  {}",
        s.dim.apply_to("Location:"),
        target.display()
    );
    println!();

    Ok(())
}

/// Remove a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub fn remove(query: &str, force: bool) -> Result<()> {
    let s = Styles::default();

    let target = find_workspace(query)?;

    let meta = read_meta(&target);
    let display_name = meta
        .as_ref()
        .map(|m| m.name.as_str())
        .unwrap_or(query);

    if !force {
        println!();
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "  Remove workspace {} at {}?",
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

    fs::remove_dir_all(&target).with_context(|| {
        format!(
            "could not remove workspace directory: {}",
            target.display()
        )
    })?;

    println!();
    println!(
        "  {} Removed workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(display_name)
    );
    println!();

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
    fn read_meta_returns_some_for_valid_workspace() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test-workspace".to_string(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = read_meta(dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "test-workspace");
    }

    #[test]
    fn read_meta_returns_none_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = read_meta(dir.path());
        assert!(result.is_none());
    }
}
