//! Scaffold a `buona.json` in the current (or given) directory.

use std::env;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::fsutil;
use crate::styles::Styles;

use super::detect::detect_build_system;
use super::types::BuildSystem;

const SCHEMA_URL: &str = "https://buona.shanley.dev/schemas/buona.schema.json";
const CONFIG_FILENAME: &str = "buona.json";

/// Options for `buona init`.
pub(crate) struct InitOptions {
    /// Force a specific build system instead of auto-detection.
    pub(crate) system: Option<BuildSystem>,
    /// Overwrite an existing `buona.json`.
    pub(crate) force: bool,
}

/// Create a `buona.json` in the current directory.
pub(crate) async fn init(options: InitOptions) -> Result<()> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    init_in_dir(&cwd, options).await
}

/// Create a `buona.json` in `dir`.
pub(super) async fn init_in_dir(dir: &Path, options: InitOptions) -> Result<()> {
    let s = Styles::default();
    let path = dir.join(CONFIG_FILENAME);

    if path.is_file() && !options.force {
        bail!(
            "{} already exists\n  \
             Re-run with --force to overwrite.",
            path.display()
        );
    }

    let system = match options.system {
        Some(system) => Some(system),
        None => detect_build_system(dir).await,
    };

    let contents = render_buona_json(system);
    fsutil::write_atomic(&path, &contents).await?;

    crate::textln!();
    crate::textln!(
        "  {} Created {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(path.display())
    );
    if let Some(system) = system {
        crate::textln!(
            "  {} system: {}",
            s.dim.apply_to("→"),
            s.cyan.apply_to(system.to_string())
        );
    } else {
        crate::textln!(
            "  {} no build system detected; system will auto-detect at runtime",
            s.dim.apply_to("→")
        );
    }
    crate::textln!();

    Ok(())
}

/// Build pretty-printed `buona.json` contents.
fn render_buona_json(system: Option<BuildSystem>) -> String {
    let mut map = serde_json::Map::new();
    map.insert("$schema".to_string(), Value::String(SCHEMA_URL.to_string()));
    if let Some(system) = system {
        map.insert("system".to_string(), json!(system));
    }
    // Stable key order: $schema first, then system.
    let value = Value::Object(map);
    format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn render_includes_schema_only_when_no_system() {
        let contents = render_buona_json(None);
        let value: Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(value["$schema"], SCHEMA_URL);
        assert!(value.get("system").is_none());
    }

    #[test]
    fn render_includes_detected_system() {
        let contents = render_buona_json(Some(BuildSystem::Npm));
        let value: Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(value["$schema"], SCHEMA_URL);
        assert_eq!(value["system"], "npm");
    }

    #[tokio::test]
    async fn init_creates_buona_json_with_detected_system() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        init_in_dir(
            dir.path(),
            InitOptions {
                system: None,
                force: false,
            },
        )
        .await
        .unwrap();

        let path = dir.path().join(CONFIG_FILENAME);
        assert!(path.is_file());
        let value: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["system"], "npm");
        assert_eq!(value["$schema"], SCHEMA_URL);
    }

    #[tokio::test]
    async fn init_respects_explicit_system_over_detection() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        init_in_dir(
            dir.path(),
            InitOptions {
                system: Some(BuildSystem::Pnpm),
                force: false,
            },
        )
        .await
        .unwrap();

        let value: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join(CONFIG_FILENAME)).unwrap())
                .unwrap();
        assert_eq!(value["system"], "pnpm");
    }

    #[tokio::test]
    async fn init_refuses_existing_without_force() {
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join(CONFIG_FILENAME);
        fs::write(&path, "{}\n").unwrap();

        let err = init_in_dir(
            dir.path(),
            InitOptions {
                system: None,
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(fs::read_to_string(path).unwrap(), "{}\n");
    }

    #[tokio::test]
    async fn init_force_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);
        fs::write(&path, "{\"system\":\"cargo\"}\n").unwrap();
        fs::write(dir.path().join("go.mod"), "module example\n").unwrap();

        init_in_dir(
            dir.path(),
            InitOptions {
                system: None,
                force: true,
            },
        )
        .await
        .unwrap();

        let value: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["system"], "go");
        assert_eq!(value["$schema"], SCHEMA_URL);
    }
}
