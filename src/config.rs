use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use dialoguer::Input;
use serde::{Deserialize, Serialize};

use crate::styles::Styles;

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
pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(rest))
    } else if path == "~" {
        dirs::home_dir().context("could not determine home directory")
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Resolve the workspace directory from the config, expanding `~`.
pub fn workspace_dir() -> Result<PathBuf> {
    let cfg = load_config()?;
    expand_tilde(&cfg.workspace_dir)
}

/// Returns the buona config directory: ~/.config/buona/
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not determine config directory")?;
    Ok(dir.join("buona"))
}

/// Returns the path to the config file: ~/.config/buona/config.json
pub fn config_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Load the config from disk. If the file doesn't exist, returns the default config.
pub fn load_config() -> Result<BuonaConfig> {
    let path = config_file_path()?;
    if path.exists() {
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("could not read config file: {}", path.display()))?;
        let config: BuonaConfig = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse config file: {}", path.display()))?;
        Ok(config)
    } else {
        Ok(BuonaConfig::default())
    }
}

/// Save the config to disk, creating parent directories if needed.
pub fn save_config(config: &BuonaConfig) -> Result<()> {
    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create config directory: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&path, json + "\n")
        .with_context(|| format!("could not write config file: {}", path.display()))?;
    Ok(())
}

/// Pretty-print the current configuration to the terminal.
pub fn print_pretty(config: &BuonaConfig) -> Result<()> {
    let s = Styles::default();

    let path = config_file_path()?;
    let file_exists = path.exists();

    println!();
    println!("  {}", s.bold.apply_to("Buona Configuration"));
    println!("  {}", s.dim.apply_to("───────────────────"));
    println!(
        "  {}  {}",
        s.cyan.apply_to("Workspace Directory:"),
        config.workspace_dir
    );
    println!();

    if !file_exists {
        println!(
            "  {}",
            s.dim.apply_to(format!(
                "No config file found. Showing defaults. Run {} to create one.",
                s.bold.apply_to("buona config setup")
            ))
        );
        println!();
    } else {
        println!(
            "  {}",
            s.dim.apply_to(format!("Config file: {}", path.display()))
        );
        println!();
    }

    Ok(())
}

/// Print the configuration as JSON.
pub fn print_json(config: &BuonaConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(config)?;
    println!("{json}");
    Ok(())
}

/// Run the interactive setup wizard.
pub fn run_setup() -> Result<()> {
    let current = load_config()?;
    let s = Styles::default();

    println!();
    println!("  {} Buona Configuration Setup", s.bold.apply_to("🔧"));
    println!("  {}", s.dim.apply_to("───────────────────────────"));
    println!();

    let workspace_dir: String = Input::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Workspace directory")))
        .default(current.workspace_dir)
        .interact_text()
        .context("failed to read input")?;

    let config = BuonaConfig { workspace_dir };

    save_config(&config)?;

    println!();
    println!(
        "  {} Configuration saved to {}",
        s.green.apply_to("✔"),
        s.dim.apply_to(config_file_path()?.display().to_string())
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_with_subpath() {
        let result = expand_tilde("~/foo").unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(result, home.join("foo"));
    }

    #[test]
    fn expand_tilde_bare() {
        let result = expand_tilde("~").unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(result, home);
    }

    #[test]
    fn expand_tilde_absolute_path_unchanged() {
        let result = expand_tilde("/usr/local/bin").unwrap();
        assert_eq!(result, PathBuf::from("/usr/local/bin"));
    }

    #[test]
    fn expand_tilde_relative_path_unchanged() {
        let result = expand_tilde("some/relative/path").unwrap();
        assert_eq!(result, PathBuf::from("some/relative/path"));
    }

    #[test]
    fn default_config_round_trips_through_serde() {
        let config = BuonaConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.workspace_dir, config.workspace_dir);
    }
}
