use std::fmt;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use dialoguer::{Input, Select};
use serde::{Deserialize, Serialize};

use crate::styles::Styles;

/// Supported IDE options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ide {
    /// Visual Studio Code
    Vscode,
    /// Cursor
    Cursor,
}

impl Ide {
    /// All variants in display order.
    pub const ALL: [Ide; 2] = [Ide::Vscode, Ide::Cursor];
}

impl Default for Ide {
    fn default() -> Self {
        Ide::Vscode
    }
}

impl fmt::Display for Ide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ide::Vscode => write!(f, "VS Code"),
            Ide::Cursor => write!(f, "Cursor"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuonaConfig {
    pub workspace_dir: String,

    /// The user's preferred IDE.
    #[serde(default)]
    pub ide: Ide,
}

impl Default for BuonaConfig {
    fn default() -> Self {
        Self {
            workspace_dir: "~/workspace".to_string(),
            ide: Ide::default(),
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
    println!(
        "  {}  {}",
        s.cyan.apply_to("IDE:"),
        config.ide
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

    let ide_options: Vec<String> = Ide::ALL.iter().map(|ide| ide.to_string()).collect();
    let ide_default = Ide::ALL
        .iter()
        .position(|&i| i == current.ide)
        .unwrap_or(0);

    let ide_index = Select::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Preferred IDE")))
        .items(&ide_options)
        .default(ide_default)
        .interact()
        .context("failed to read input")?;

    let ide = Ide::ALL[ide_index];

    let config = BuonaConfig {
        workspace_dir,
        ide,
    };

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
        assert_eq!(deserialized.ide, config.ide);
    }

    #[test]
    fn default_ide_is_vscode() {
        assert_eq!(Ide::default(), Ide::Vscode);
    }

    #[test]
    fn ide_display_vscode() {
        assert_eq!(Ide::Vscode.to_string(), "VS Code");
    }

    #[test]
    fn ide_display_cursor() {
        assert_eq!(Ide::Cursor.to_string(), "Cursor");
    }

    #[test]
    fn ide_serializes_to_lowercase() {
        assert_eq!(serde_json::to_string(&Ide::Vscode).unwrap(), "\"vscode\"");
        assert_eq!(serde_json::to_string(&Ide::Cursor).unwrap(), "\"cursor\"");
    }

    #[test]
    fn ide_deserializes_from_lowercase() {
        let vscode: Ide = serde_json::from_str("\"vscode\"").unwrap();
        assert_eq!(vscode, Ide::Vscode);

        let cursor: Ide = serde_json::from_str("\"cursor\"").unwrap();
        assert_eq!(cursor, Ide::Cursor);
    }

    #[test]
    fn config_without_ide_field_defaults_to_vscode() {
        let json = r#"{"workspace_dir": "~/workspace"}"#;
        let config: BuonaConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.ide, Ide::Vscode);
    }

    #[test]
    fn config_with_ide_cursor_round_trips() {
        let config = BuonaConfig {
            workspace_dir: "~/work".to_string(),
            ide: Ide::Cursor,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ide, Ide::Cursor);
    }
}
