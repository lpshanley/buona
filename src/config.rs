//! Global configuration management.
//!
//! Handles reading, writing, and interactive setup of the global config file
//! at `~/.config/buona/config.json`.

use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result};
use dialoguer::{Input, Select};
use serde::{Deserialize, Serialize};

use crate::styles::Styles;

/// Supported IDE options.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Ide {
    /// Visual Studio Code
    #[default]
    Vscode,
    /// Cursor
    Cursor,
    /// Windsurf
    Windsurf,
}

impl Ide {
    /// All variants in display order.
    pub(crate) const ALL: [Ide; 3] = [Ide::Vscode, Ide::Cursor, Ide::Windsurf];

    /// Returns the CLI command name used to launch this editor.
    pub(crate) fn command(&self) -> &'static str {
        match self {
            Ide::Vscode => "code",
            Ide::Cursor => "cursor",
            Ide::Windsurf => "windsurf",
        }
    }
}

impl fmt::Display for Ide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ide::Vscode => write!(f, "VS Code"),
            Ide::Cursor => write!(f, "Cursor"),
            Ide::Windsurf => write!(f, "Windsurf"),
        }
    }
}

/// Git tracking modes for workspaces.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GitTracking {
    /// Each package has its own .git directory (default).
    #[default]
    Package,
    /// A single .git at the workspace root tracks everything.
    Workspace,
}

impl GitTracking {
    /// All variants in display order.
    pub(crate) const ALL: [GitTracking; 2] = [GitTracking::Package, GitTracking::Workspace];
}

impl fmt::Display for GitTracking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitTracking::Package => write!(f, "Package-level"),
            GitTracking::Workspace => write!(f, "Workspace-level"),
        }
    }
}

/// Supported git protocols.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GitProtocol {
    /// SSH (e.g. git@github.com:org/repo.git)
    #[default]
    Ssh,
    /// HTTPS (e.g. https://github.com/org/repo.git)
    Https,
}

impl GitProtocol {
    /// All variants in display order.
    pub(crate) const ALL: [GitProtocol; 2] = [GitProtocol::Ssh, GitProtocol::Https];
}

impl fmt::Display for GitProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitProtocol::Ssh => write!(f, "SSH"),
            GitProtocol::Https => write!(f, "HTTPS"),
        }
    }
}

/// Default git settings (host, organization, protocol).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GitConfig {
    /// The git host (e.g. "github.com" or a GitHub Enterprise host).
    #[serde(default = "default_git_host")]
    pub(crate) host: String,

    /// The default organization on the host (e.g. "my-org").
    #[serde(default)]
    pub(crate) organization: String,

    /// The protocol used to clone/push (SSH or HTTPS).
    #[serde(default)]
    pub(crate) protocol: GitProtocol,

    /// Default git tracking mode for workspaces.
    #[serde(default)]
    pub(crate) tracking: GitTracking,
}

fn default_git_host() -> String {
    "github.com".to_string()
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            host: default_git_host(),
            organization: String::new(),
            protocol: GitProtocol::default(),
            tracking: GitTracking::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BuonaConfig {
    pub(crate) workspace_dir: String,

    /// The user's preferred IDE.
    #[serde(default)]
    pub(crate) ide: Ide,

    /// Default git settings.
    #[serde(default)]
    pub(crate) git: GitConfig,
}

impl Default for BuonaConfig {
    fn default() -> Self {
        Self {
            workspace_dir: "~/workspace".to_string(),
            ide: Ide::default(),
            git: GitConfig::default(),
        }
    }
}

/// Expand a leading `~` to the user's home directory.
pub(crate) fn expand_tilde(path: &str) -> Result<PathBuf> {
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
pub(crate) async fn workspace_dir() -> Result<PathBuf> {
    let cfg = load_config().await?;
    expand_tilde(&cfg.workspace_dir)
}

/// Returns the buona config directory: ~/.config/buona/
pub(crate) fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not determine config directory")?;
    Ok(dir.join("buona"))
}

/// Returns the path to the config file: ~/.config/buona/config.json
pub(crate) fn config_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Load the config from disk. If the file doesn't exist, returns the default config.
pub(crate) async fn load_config() -> Result<BuonaConfig> {
    let path = config_file_path()?;
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let config: BuonaConfig = serde_json::from_str(&contents)
                .with_context(|| format!("could not parse config file: {}", path.display()))?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BuonaConfig::default()),
        Err(e) => Err(e).with_context(|| format!("could not read config file: {}", path.display())),
    }
}

/// Save the config to disk, creating parent directories if needed.
pub(crate) async fn save_config(config: &BuonaConfig) -> Result<()> {
    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("could not create config directory: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&path, json + "\n")
        .await
        .with_context(|| format!("could not write config file: {}", path.display()))?;
    Ok(())
}

/// Pretty-print the current configuration to the terminal.
pub(crate) async fn print_pretty(config: &BuonaConfig) -> Result<()> {
    let s = Styles::default();

    let path = config_file_path()?;
    let file_exists = tokio::fs::try_exists(&path).await.unwrap_or(false);

    println!();
    println!("  {}", s.bold.apply_to("Buona Configuration"));
    println!("  {}", s.dim.apply_to("───────────────────"));
    println!(
        "  {}  {}",
        s.cyan.apply_to("Workspace Directory:"),
        config.workspace_dir
    );
    println!("  {}  {}", s.cyan.apply_to("IDE:"), config.ide);
    println!();
    println!("  {}", s.bold.apply_to("Git Defaults"));
    println!("  {}", s.dim.apply_to("───────────────────"));
    println!("  {}  {}", s.cyan.apply_to("Host:"), config.git.host);
    println!(
        "  {}  {}",
        s.cyan.apply_to("Organization:"),
        if config.git.organization.is_empty() {
            "(not set)".to_string()
        } else {
            config.git.organization.clone()
        }
    );
    println!(
        "  {}  {}",
        s.cyan.apply_to("Protocol:"),
        config.git.protocol
    );
    println!(
        "  {}  {}",
        s.cyan.apply_to("Tracking:"),
        config.git.tracking
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
pub(crate) fn print_json(config: &BuonaConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(config)?;
    println!("{json}");
    Ok(())
}

/// Run the interactive setup wizard.
pub(crate) async fn run_setup() -> Result<()> {
    let current = load_config().await?;
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
    let ide_default = Ide::ALL.iter().position(|&i| i == current.ide).unwrap_or(0);

    let ide_index = Select::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Preferred IDE")))
        .items(&ide_options)
        .default(ide_default)
        .interact()
        .context("failed to read input")?;

    let ide = Ide::ALL[ide_index];

    println!();
    println!("  {} Git Defaults", s.bold.apply_to("🔗"));
    println!("  {}", s.dim.apply_to("───────────────────────────"));
    println!();

    let git_host: String = Input::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Git host")))
        .default(current.git.host)
        .interact_text()
        .context("failed to read input")?;

    let git_org: String = Input::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Default organization")))
        .default(current.git.organization)
        .allow_empty(true)
        .interact_text()
        .context("failed to read input")?;

    let protocol_options: Vec<String> = GitProtocol::ALL.iter().map(|p| p.to_string()).collect();
    let protocol_default = GitProtocol::ALL
        .iter()
        .position(|&p| p == current.git.protocol)
        .unwrap_or(0);

    let protocol_index = Select::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Git protocol")))
        .items(&protocol_options)
        .default(protocol_default)
        .interact()
        .context("failed to read input")?;

    let git_protocol = GitProtocol::ALL[protocol_index];

    let tracking_options: Vec<String> = GitTracking::ALL.iter().map(|t| t.to_string()).collect();
    let tracking_default = GitTracking::ALL
        .iter()
        .position(|&t| t == current.git.tracking)
        .unwrap_or(0);

    let tracking_index = Select::new()
        .with_prompt(format!("  {}", s.bold.apply_to("Git tracking mode")))
        .items(&tracking_options)
        .default(tracking_default)
        .interact()
        .context("failed to read input")?;

    let git_tracking = GitTracking::ALL[tracking_index];

    let config = BuonaConfig {
        workspace_dir,
        ide,
        git: GitConfig {
            host: git_host,
            organization: git_org,
            protocol: git_protocol,
            tracking: git_tracking,
        },
    };

    save_config(&config).await?;

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
        assert_eq!(deserialized.git, config.git);
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
            git: GitConfig::default(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ide, Ide::Cursor);
    }

    // ── GitProtocol tests ──────────────────────────────────────────

    #[test]
    fn default_git_protocol_is_ssh() {
        assert_eq!(GitProtocol::default(), GitProtocol::Ssh);
    }

    #[test]
    fn git_protocol_display_ssh() {
        assert_eq!(GitProtocol::Ssh.to_string(), "SSH");
    }

    #[test]
    fn git_protocol_display_https() {
        assert_eq!(GitProtocol::Https.to_string(), "HTTPS");
    }

    #[test]
    fn git_protocol_serializes_to_lowercase() {
        assert_eq!(serde_json::to_string(&GitProtocol::Ssh).unwrap(), "\"ssh\"");
        assert_eq!(
            serde_json::to_string(&GitProtocol::Https).unwrap(),
            "\"https\""
        );
    }

    #[test]
    fn git_protocol_deserializes_from_lowercase() {
        let ssh: GitProtocol = serde_json::from_str("\"ssh\"").unwrap();
        assert_eq!(ssh, GitProtocol::Ssh);

        let https: GitProtocol = serde_json::from_str("\"https\"").unwrap();
        assert_eq!(https, GitProtocol::Https);
    }

    // ── GitConfig tests ────────────────────────────────────────────

    #[test]
    fn default_git_config_has_github_host() {
        let git = GitConfig::default();
        assert_eq!(git.host, "github.com");
        assert_eq!(git.organization, "");
        assert_eq!(git.protocol, GitProtocol::Ssh);
    }

    #[test]
    fn git_config_round_trips_through_serde() {
        let git = GitConfig {
            host: "git.example.com".to_string(),
            organization: "my-org".to_string(),
            protocol: GitProtocol::Https,
            tracking: GitTracking::Package,
        };
        let json = serde_json::to_string(&git).unwrap();
        let deserialized: GitConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, git);
    }

    #[test]
    fn config_without_git_field_gets_defaults() {
        let json = r#"{"workspace_dir": "~/workspace"}"#;
        let config: BuonaConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.git, GitConfig::default());
    }

    #[test]
    fn config_with_partial_git_gets_defaults_for_missing() {
        let json = r#"{"workspace_dir": "~/workspace", "git": {"host": "git.corp.com"}}"#;
        let config: BuonaConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.git.host, "git.corp.com");
        assert_eq!(config.git.organization, "");
        assert_eq!(config.git.protocol, GitProtocol::Ssh);
    }

    // ── Ide::command tests ────────────────────────────────────────

    #[test]
    fn ide_command_vscode() {
        assert_eq!(Ide::Vscode.command(), "code");
    }

    #[test]
    fn ide_command_cursor() {
        assert_eq!(Ide::Cursor.command(), "cursor");
    }

    #[test]
    fn config_with_full_git_round_trips() {
        let config = BuonaConfig {
            workspace_dir: "~/work".to_string(),
            ide: Ide::Cursor,
            git: GitConfig {
                host: "github.example.com".to_string(),
                organization: "engineering".to_string(),
                protocol: GitProtocol::Https,
                tracking: GitTracking::Workspace,
            },
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.git.host, "github.example.com");
        assert_eq!(deserialized.git.organization, "engineering");
        assert_eq!(deserialized.git.protocol, GitProtocol::Https);
        assert_eq!(deserialized.git.tracking, GitTracking::Workspace);
    }

    // ── GitTracking tests ────────────────────────────────────────

    #[test]
    fn default_git_tracking_is_package() {
        assert_eq!(GitTracking::default(), GitTracking::Package);
    }

    #[test]
    fn git_tracking_display_package() {
        assert_eq!(GitTracking::Package.to_string(), "Package-level");
    }

    #[test]
    fn git_tracking_display_workspace() {
        assert_eq!(GitTracking::Workspace.to_string(), "Workspace-level");
    }

    #[test]
    fn git_tracking_serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&GitTracking::Package).unwrap(),
            "\"package\""
        );
        assert_eq!(
            serde_json::to_string(&GitTracking::Workspace).unwrap(),
            "\"workspace\""
        );
    }

    #[test]
    fn git_tracking_deserializes_from_lowercase() {
        let package: GitTracking = serde_json::from_str("\"package\"").unwrap();
        assert_eq!(package, GitTracking::Package);

        let workspace: GitTracking = serde_json::from_str("\"workspace\"").unwrap();
        assert_eq!(workspace, GitTracking::Workspace);
    }

    #[test]
    fn config_without_tracking_field_defaults_to_package() {
        let json = r#"{"workspace_dir": "~/workspace", "git": {"host": "github.com"}}"#;
        let config: BuonaConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.git.tracking, GitTracking::Package);
    }
}
