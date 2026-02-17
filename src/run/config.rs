//! Per-package configuration loaded from `buona.json`.
//!
//! This is distinct from the global `~/.config/buona/config.json` managed by
//! `crate::config`. A `buona.json` lives in the package root and controls how
//! `buona run` resolves commands for that package.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};

use super::types::BuildSystem;

/// Explicit hook value in `buona.json`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(super) enum HookValue {
    /// A shell command/script string.
    Script(String),
    /// An explicit argv array: [program, arg1, ...].
    Argv(Vec<String>),
}

/// Per-command override in `buona.json`.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct CommandConfig {
    /// Override the build system for just this command.
    #[serde(default)]
    pub(super) system: Option<BuildSystem>,

    /// Full exec override: replaces the entire argv.
    #[serde(default)]
    pub(super) exec: Option<Vec<String>>,
}

/// Typed global system selection from `buona.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ConfigSystem {
    #[default]
    Auto,
    Fixed(BuildSystem),
}

impl<'de> Deserialize<'de> for ConfigSystem {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw == "auto" {
            return Ok(Self::Auto);
        }

        serde_json::from_value::<BuildSystem>(serde_json::Value::String(raw.clone()))
            .map(Self::Fixed)
            .map_err(|_| DeError::custom(format!("unknown build system \"{raw}\"")))
    }
}

/// Per-package configuration loaded from `buona.json`.
///
/// Example:
/// ```json
/// {
///   "system": "cargo",
///   "commands": {
///     "build": { "system": "make" },
///     "test": { "exec": ["pnpm", "run", "custom-test"] }
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub(super) struct BuonaRunConfig {
    /// Global build system for this package.
    /// Defaults to `auto` if omitted.
    #[serde(default = "default_system")]
    pub(super) system: ConfigSystem,

    /// Per-command overrides.
    #[serde(default)]
    pub(super) commands: HashMap<String, CommandConfig>,

    /// Directory to scan for convention-based hook scripts.
    /// Relative paths are resolved from the package root.
    #[serde(default = "default_hooks_dir", rename = "hooksDir")]
    pub(super) hooks_dir: String,

    /// Explicit hook definitions. Keys are `pre<command>` or `post<command>`.
    /// Values can be either:
    /// - string: a recognized system name or a literal shell command
    /// - array: explicit argv `[program, arg1, ...]`
    #[serde(default)]
    pub(super) hooks: HashMap<String, HookValue>,
}

fn default_system() -> ConfigSystem {
    ConfigSystem::Auto
}

fn default_hooks_dir() -> String {
    ".buona/hooks".to_string()
}

/// The filename for per-package configuration.
pub(super) const PACKAGE_CONFIG_FILE: &str = "buona.json";

/// Load per-package config from a directory.
///
/// Returns `Ok(None)` if `buona.json` does not exist.
/// Returns `Err` if it exists but is malformed.
pub(super) async fn load_package_config(dir: &Path) -> Result<Option<BuonaRunConfig>> {
    let path = dir.join(PACKAGE_CONFIG_FILE);
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => {
            let config: BuonaRunConfig = serde_json::from_str(&contents)
                .with_context(|| format!("invalid {} in {}", PACKAGE_CONFIG_FILE, dir.display()))?;
            Ok(Some(config))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!(
                "could not read {} in {}",
                PACKAGE_CONFIG_FILE,
                dir.display()
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_missing_config_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = load_package_config(dir.path()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_valid_config_with_system() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{ "system": "cargo" }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.system, ConfigSystem::Fixed(BuildSystem::Cargo));
        assert!(config.commands.is_empty());
    }

    #[tokio::test]
    async fn load_valid_config_with_commands() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{
                "system": "auto",
                "commands": {
                    "build": { "system": "make" },
                    "test": { "exec": ["pnpm", "run", "custom-test"] }
                }
            }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.system, ConfigSystem::Auto);
        assert_eq!(config.commands.len(), 2);

        let build = &config.commands["build"];
        assert_eq!(build.system, Some(BuildSystem::Make));
        assert!(build.exec.is_none());

        let test = &config.commands["test"];
        assert!(test.system.is_none());
        assert_eq!(test.exec.as_ref().unwrap(), &["pnpm", "run", "custom-test"]);
    }

    #[tokio::test]
    async fn load_defaults_system_to_auto() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(PACKAGE_CONFIG_FILE), r#"{}"#).unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.system, ConfigSystem::Auto);
    }

    #[tokio::test]
    async fn load_defaults_hooks_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(PACKAGE_CONFIG_FILE), r#"{}"#).unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.hooks_dir, ".buona/hooks");
        assert!(config.hooks.is_empty());
    }

    #[tokio::test]
    async fn load_config_with_hooks_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{ "hooksDir": ".custom/hooks" }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.hooks_dir, ".custom/hooks");
    }

    #[tokio::test]
    async fn load_config_with_hooks() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{
                "hooks": {
                    "prebuild": "./scripts/gen.sh",
                    "posttest": "docker compose down"
                }
            }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.hooks.len(), 2);
        assert_eq!(
            config.hooks["prebuild"],
            HookValue::Script("./scripts/gen.sh".to_string())
        );
        assert_eq!(
            config.hooks["posttest"],
            HookValue::Script("docker compose down".to_string())
        );
    }

    #[tokio::test]
    async fn load_config_with_hook_argv() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{
                "hooks": {
                    "prebuild": ["pnpm", "run", "build"]
                }
            }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(
            config.hooks["prebuild"],
            HookValue::Argv(vec![
                "pnpm".to_string(),
                "run".to_string(),
                "build".to_string(),
            ])
        );
    }

    #[tokio::test]
    async fn existing_config_still_deserializes_with_new_fields() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{ "system": "cargo", "commands": { "build": { "system": "make" } } }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).await.unwrap().unwrap();
        assert_eq!(config.system, ConfigSystem::Fixed(BuildSystem::Cargo));
        assert_eq!(config.hooks_dir, ".buona/hooks");
        assert!(config.hooks.is_empty());
    }

    #[tokio::test]
    async fn load_malformed_config_returns_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(PACKAGE_CONFIG_FILE), "not valid json {{{").unwrap();

        let result = load_package_config(dir.path()).await;
        assert!(result.is_err());
    }
}
