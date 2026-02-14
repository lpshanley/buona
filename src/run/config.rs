//! Per-package configuration loaded from `buona.json`.
//!
//! This is distinct from the global `~/.config/buona/config.json` managed by
//! `crate::config`. A `buona.json` lives in the package root and controls how
//! `buona run` resolves commands for that package.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::types::BuildSystem;

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
    /// Global build system for this package ("auto" or a system name).
    /// Defaults to "auto" if omitted.
    #[serde(default = "default_system_str")]
    pub(super) system: String,

    /// Per-command overrides.
    #[serde(default)]
    pub(super) commands: HashMap<String, CommandConfig>,
}

fn default_system_str() -> String {
    "auto".to_string()
}

/// The filename for per-package configuration.
pub(super) const PACKAGE_CONFIG_FILE: &str = "buona.json";

/// Load per-package config from a directory.
///
/// Returns `Ok(None)` if `buona.json` does not exist.
/// Returns `Err` if it exists but is malformed.
pub(super) fn load_package_config(dir: &Path) -> Result<Option<BuonaRunConfig>> {
    let path = dir.join(PACKAGE_CONFIG_FILE);
    match fs::read_to_string(&path) {
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

    #[test]
    fn load_missing_config_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = load_package_config(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_valid_config_with_system() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            r#"{ "system": "cargo" }"#,
        )
        .unwrap();

        let config = load_package_config(dir.path()).unwrap().unwrap();
        assert_eq!(config.system, "cargo");
        assert!(config.commands.is_empty());
    }

    #[test]
    fn load_valid_config_with_commands() {
        let dir = TempDir::new().unwrap();
        fs::write(
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

        let config = load_package_config(dir.path()).unwrap().unwrap();
        assert_eq!(config.system, "auto");
        assert_eq!(config.commands.len(), 2);

        let build = &config.commands["build"];
        assert_eq!(build.system, Some(BuildSystem::Make));
        assert!(build.exec.is_none());

        let test = &config.commands["test"];
        assert!(test.system.is_none());
        assert_eq!(
            test.exec.as_ref().unwrap(),
            &["pnpm", "run", "custom-test"]
        );
    }

    #[test]
    fn load_defaults_system_to_auto() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(PACKAGE_CONFIG_FILE), r#"{}"#).unwrap();

        let config = load_package_config(dir.path()).unwrap().unwrap();
        assert_eq!(config.system, "auto");
    }

    #[test]
    fn load_malformed_config_returns_error() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(PACKAGE_CONFIG_FILE),
            "not valid json {{{",
        )
        .unwrap();

        let result = load_package_config(dir.path());
        assert!(result.is_err());
    }
}
