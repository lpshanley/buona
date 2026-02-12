use std::fs;
use std::path::PathBuf;

use console::Style;
use dialoguer::Input;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BuonaConfig {
    pub workspace_dir: String,
}

impl Default for BuonaConfig {
    fn default() -> Self {
        Self {
            workspace_dir: "~/workspace".to_string(),
        }
    }
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .expect("Could not determine home directory")
            .join(rest)
    } else if path == "~" {
        dirs::home_dir().expect("Could not determine home directory")
    } else {
        PathBuf::from(path)
    }
}

/// Resolve the workspace directory from the config, expanding `~`.
pub fn workspace_dir() -> PathBuf {
    let cfg = load_config();
    expand_tilde(&cfg.workspace_dir)
}

/// Returns the buona config directory: ~/.config/buona/
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("buona")
}

/// Returns the path to the config file: ~/.config/buona/config.json
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Load the config from disk. If the file doesn't exist, returns the default config.
pub fn load_config() -> BuonaConfig {
    let path = config_file_path();
    if path.exists() {
        let contents = fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Warning: Could not read config file: {e}");
            String::new()
        });
        serde_json::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("Warning: Could not parse config file: {e}");
            eprintln!("Using default configuration.");
            BuonaConfig::default()
        })
    } else {
        BuonaConfig::default()
    }
}

/// Save the config to disk, creating parent directories if needed.
pub fn save_config(config: &BuonaConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, json + "\n")?;
    Ok(())
}

/// Pretty-print the current configuration to the terminal.
pub fn print_pretty(config: &BuonaConfig) {
    let bold = Style::new().bold();
    let dim = Style::new().dim();
    let cyan = Style::new().cyan().bold();

    let path = config_file_path();
    let file_exists = path.exists();

    println!();
    println!("  {}", bold.apply_to("Buona Configuration"));
    println!("  {}", dim.apply_to("───────────────────"));
    println!(
        "  {}  {}",
        cyan.apply_to("Workspace Directory:"),
        config.workspace_dir
    );
    println!();

    if !file_exists {
        println!(
            "  {}",
            dim.apply_to(format!(
                "No config file found. Showing defaults. Run {} to create one.",
                Style::new().bold().apply_to("buona config --setup")
            ))
        );
        println!();
    } else {
        println!(
            "  {}",
            dim.apply_to(format!("Config file: {}", path.display()))
        );
        println!();
    }
}

/// Print the configuration as JSON.
pub fn print_json(config: &BuonaConfig) {
    let json = serde_json::to_string_pretty(config).expect("Failed to serialize config");
    println!("{json}");
}

/// Run the interactive setup wizard.
pub fn run_setup() {
    let current = load_config();
    let bold = Style::new().bold();
    let dim = Style::new().dim();
    let green = Style::new().green().bold();

    println!();
    println!("  {} Buona Configuration Setup", bold.apply_to("🔧"));
    println!("  {}", dim.apply_to("───────────────────────────"));
    println!();

    let workspace_dir: String = Input::new()
        .with_prompt(format!(
            "  {}",
            bold.apply_to("Workspace directory")
        ))
        .default(current.workspace_dir)
        .interact_text()
        .expect("Failed to read input");

    let config = BuonaConfig { workspace_dir };

    match save_config(&config) {
        Ok(()) => {
            println!();
            println!(
                "  {} Configuration saved to {}",
                green.apply_to("✔"),
                dim.apply_to(config_file_path().display().to_string())
            );
            println!();
        }
        Err(e) => {
            eprintln!();
            eprintln!("  Error saving configuration: {e}");
            eprintln!();
            std::process::exit(1);
        }
    }
}
