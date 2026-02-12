use std::fs;

use console::Style;

use crate::config;

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

    let mut workspaces: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.file_name().to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect();

    workspaces.sort();

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
        for name in &workspaces {
            println!("  {}  {name}", cyan.apply_to("•"));
        }
    }

    println!();
}
