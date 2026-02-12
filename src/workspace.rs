use std::fs;
use std::path::PathBuf;

use console::Style;
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};

use crate::config;

const WORKSPACE_FILE: &str = "buona.workspace.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub name: String,
}

/// Read workspace metadata from a directory, if a `buona.workspace.json` exists.
pub fn read_meta(dir: &PathBuf) -> Option<WorkspaceMeta> {
    let path = dir.join(WORKSPACE_FILE);
    let contents = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Find a workspace by name or directory name. Returns the resolved path.
fn find_workspace(query: &str) -> Option<PathBuf> {
    let workspace_dir = config::workspace_dir();

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct).is_some() {
        return Some(direct);
    }

    // Otherwise, search by workspace name in metadata
    let entries = fs::read_dir(&workspace_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().ok()?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path) {
                if meta.name == query {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// List all workspaces (directories) found in the configured workspace directory.
pub fn list() {
    let workspace_dir = config::workspace_dir();
    let bold = Style::new().bold();
    let dim = Style::new().dim();
    let cyan = Style::new().cyan().bold();

    println!();
    println!("  {}", bold.apply_to("Workspaces"));
    println!("  {}", dim.apply_to("──────────"));

    if !workspace_dir.exists() {
        eprintln!(
            "  Workspace directory does not exist: {}",
            workspace_dir.display()
        );
        eprintln!(
            "  Run {} to configure it.",
            bold.apply_to("buona config --setup")
        );
        println!();
        std::process::exit(1);
    }

    let entries = match fs::read_dir(&workspace_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!(
                "  Could not read workspace directory {}: {e}",
                workspace_dir.display()
            );
            println!();
            std::process::exit(1);
        }
    };

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
            dim.apply_to(format!(
                "No workspaces found in {}",
                workspace_dir.display()
            ))
        );
    } else {
        println!(
            "  {}  {}",
            dim.apply_to("Directory:"),
            workspace_dir.display()
        );
        println!();
        for (dir_name, meta) in &workspaces {
            if meta.name != *dir_name {
                println!(
                    "  {}  {} {}",
                    cyan.apply_to("•"),
                    meta.name,
                    dim.apply_to(format!("({dir_name})"))
                );
            } else {
                println!("  {}  {dir_name}", cyan.apply_to("•"));
            }
        }
    }

    println!();
}

/// Create a new workspace directory. Writes a `buona.workspace.json` marker
/// file with the workspace name. If `name` is not provided, the directory name
/// is used.
pub fn create(path: &str, name: Option<&str>) {
    let bold = Style::new().bold();
    let dim = Style::new().dim();
    let green = Style::new().green().bold();

    // Resolve the target directory
    let target: PathBuf = if PathBuf::from(path).is_absolute() {
        PathBuf::from(path)
    } else {
        config::workspace_dir().join(path)
    };

    // Derive the workspace name
    let ws_name = name
        .map(String::from)
        .unwrap_or_else(|| {
            target
                .file_name()
                .expect("Could not determine directory name from path")
                .to_string_lossy()
                .into_owned()
        });

    if target.exists() {
        eprintln!(
            "  Directory already exists: {}",
            target.display()
        );
        std::process::exit(1);
    }

    // Create the workspace directory (and any parent directories)
    if let Err(e) = fs::create_dir_all(&target) {
        eprintln!(
            "  Could not create workspace directory {}: {e}",
            target.display()
        );
        std::process::exit(1);
    }

    // Write the workspace metadata file
    let meta = WorkspaceMeta { name: ws_name.clone() };
    let meta_path = target.join(WORKSPACE_FILE);
    match serde_json::to_string_pretty(&meta) {
        Ok(json) => {
            if let Err(e) = fs::write(&meta_path, json + "\n") {
                eprintln!("  Warning: Could not write {WORKSPACE_FILE}: {e}");
            }
        }
        Err(e) => {
            eprintln!("  Warning: Could not serialize workspace metadata: {e}");
        }
    }

    println!();
    println!(
        "  {} Created workspace {}",
        green.apply_to("✔"),
        bold.apply_to(&ws_name)
    );
    println!(
        "  {}  {}",
        dim.apply_to("Location:"),
        target.display()
    );
    println!();
}

/// Remove a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub fn remove(query: &str, force: bool) {
    let bold = Style::new().bold();
    let dim = Style::new().dim();
    let green = Style::new().green().bold();
    let red = Style::new().red().bold();

    let target = match find_workspace(query) {
        Some(path) => path,
        None => {
            eprintln!(
                "  {} No workspace found matching \"{}\"",
                red.apply_to("✗"),
                query
            );
            std::process::exit(1);
        }
    };

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
                bold.apply_to(display_name),
                dim.apply_to(target.display().to_string())
            ))
            .default(false)
            .interact()
            .expect("Failed to read input");

        if !confirmed {
            println!("  Aborted.");
            println!();
            return;
        }
    }

    if let Err(e) = fs::remove_dir_all(&target) {
        eprintln!(
            "  {} Could not remove workspace directory {}: {e}",
            red.apply_to("✗"),
            target.display()
        );
        std::process::exit(1);
    }

    println!();
    println!(
        "  {} Removed workspace {}",
        green.apply_to("✔"),
        bold.apply_to(display_name)
    );
    println!();
}
