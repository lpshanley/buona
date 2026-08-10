//! Safe discovery command for humans and automation.

use std::env;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::config;
use crate::styles::Styles;
use crate::workspace;

use super::detect::detect_all_systems;
use super::error::RunError;
use super::ops::find_optional_workspace_root;
use super::output::target_plan_json;
use super::planner::resolve_target_run_plan;
use super::targets::{resolve_local_target, resolve_targets};

const STANDARD_COMMANDS: &[&str] = &[
    "install", "build", "run", "test", "lint", "fmt", "clean", "publish", "bench", "doc", "dev",
];

pub(crate) async fn inspect(target_name: Option<String>) -> Result<()> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    let document = inspect_document(&cwd, target_name.as_deref()).await?;

    if crate::output::is_json() {
        return crate::output::print_json(&document);
    }

    print_text(&document);
    Ok(())
}

async fn inspect_document(cwd: &Path, target_name: Option<&str>) -> Result<Value> {
    let workspace_root = find_optional_workspace_root(cwd).await?;

    if workspace_root.is_none() && target_name.is_some() {
        return Err(
            RunError::ConfigError("--target requires a buona workspace".to_string()).into(),
        );
    }

    let target = match workspace_root {
        Some(ref root) => {
            let names = target_name
                .map(ToOwned::to_owned)
                .into_iter()
                .collect::<Vec<_>>();
            resolve_targets(cwd, root, &names, false)
                .await?
                .into_iter()
                .next()
                .context("no execution target resolved")?
        }
        None => resolve_local_target(cwd).await?,
    };

    let detections = detect_all_systems(&target.dir).await;
    let mut commands = serde_json::Map::new();
    for command in STANDARD_COMMANDS {
        let plan = resolve_target_run_plan(target.clone(), command, &[], None).await?;
        commands.insert((*command).to_string(), target_plan_json(&plan));
    }

    let global_config_path = config::config_file_path()?;
    let target_config_path = target.dir.join("buona.json");

    let (workspace, packages, workspace_config_path) = match workspace_root.as_ref() {
        Some(root) => {
            let metadata_path = root.join("buona.workspace.json");
            let metadata = read_json_if_present(&metadata_path).await?;
            let packages = workspace::list_package_names(root).await?;
            (
                Some(json!({
                    "root": root,
                    "metadata_file": metadata_path,
                    "metadata": metadata,
                })),
                packages,
                Some(root.join("buona.json")),
            )
        }
        None => (None, Vec::new(), None),
    };

    let mut config_sources = vec![config_source("global", &global_config_path)];
    if let Some(path) = workspace_config_path.as_ref()
        && path != &target_config_path
    {
        config_sources.push(config_source("workspace-target", path));
    }
    config_sources.push(config_source("selected-target", &target_config_path));

    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "cwd": cwd,
        "workspace": workspace,
        "target": {
            "name": target.label(),
            "kind": if target.is_workspace_root { "workspace-root" } else { "package" },
            "directory": target.dir,
        },
        "packages": packages,
        "config_sources": config_sources,
        "detected_systems": detections.iter().map(|detection| json!({
            "system": detection.system.to_string(),
            "marker": detection.marker,
        })).collect::<Vec<_>>(),
        "commands": commands,
    }))
}

fn config_source(scope: &str, path: &std::path::Path) -> Value {
    json!({
        "scope": scope,
        "path": path,
        "exists": path.is_file(),
    })
}

async fn read_json_if_present(path: &std::path::Path) -> Result<Option<Value>> {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => Ok(Some(
            serde_json::from_str(&contents)
                .with_context(|| format!("could not parse {}", path.display()))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn print_text(document: &Value) {
    let s = Styles::default();
    crate::textln!();
    crate::textln!("  {}", s.bold.apply_to("Buona Inspection"));
    crate::textln!("  {}", s.dim.apply_to("────────────────"));
    crate::textln!(
        "  {}  {}",
        s.cyan.apply_to("Version:"),
        document["version"].as_str().unwrap_or("unknown")
    );
    crate::textln!(
        "  {}  {}",
        s.cyan.apply_to("Target:"),
        document["target"]["name"].as_str().unwrap_or("unknown")
    );
    crate::textln!(
        "  {}  {}",
        s.cyan.apply_to("Directory:"),
        document["target"]["directory"]
            .as_str()
            .unwrap_or("unknown")
    );
    let detected = document["detected_systems"]
        .as_array()
        .and_then(|values| values.first())
        .and_then(|value| value["system"].as_str())
        .unwrap_or("none");
    crate::textln!("  {}  {}", s.cyan.apply_to("Detected system:"), detected);
    crate::textln!(
        "  {}  {}",
        s.cyan.apply_to("Packages:"),
        document["packages"].as_array().map_or(0, Vec::len)
    );
    crate::textln!();
    crate::textln!(
        "  {}",
        s.dim
            .apply_to("Use `--output json` for resolved commands, hooks, and config sources.")
    );
    crate::textln!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }

    #[tokio::test]
    async fn standalone_document_resolves_detection_and_commands() {
        let cwd = fixture("tests/fixtures/systems/cargo");
        let document = inspect_document(&cwd, None).await.unwrap();

        assert!(document["workspace"].is_null());
        assert_eq!(document["target"]["name"], "cargo");
        assert_eq!(document["target"]["kind"], "package");
        assert_eq!(document["detected_systems"][0]["system"], "cargo");
        assert_eq!(document["commands"].as_object().unwrap().len(), 11);
        assert_eq!(document["commands"]["test"]["plan"]["program"], "cargo");
        assert_eq!(document["config_sources"].as_array().unwrap().len(), 2);

        print_text(&document);
    }

    #[tokio::test]
    async fn workspace_document_includes_metadata_packages_and_config_sources() {
        let cwd = fixture("tests/fixtures/workspace");
        let document = inspect_document(&cwd, Some("cargo-app")).await.unwrap();

        assert_eq!(document["workspace"]["metadata"]["name"], "agent-fixture");
        assert_eq!(document["target"]["name"], "cargo-app");
        assert_eq!(document["target"]["kind"], "package");
        assert_eq!(document["packages"], json!(["cargo-app", "node-app"]));
        assert_eq!(document["config_sources"].as_array().unwrap().len(), 3);
        assert_eq!(document["detected_systems"][0]["system"], "cargo");
    }

    #[tokio::test]
    async fn explicit_target_requires_workspace() {
        let cwd = fixture("tests/fixtures/systems/cargo");
        let error = inspect_document(&cwd, Some("root")).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--target requires a buona workspace")
        );
    }

    #[tokio::test]
    async fn optional_json_reader_handles_present_missing_and_malformed_files() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("metadata.json");

        assert_eq!(read_json_if_present(&path).await.unwrap(), None);

        tokio::fs::write(&path, r#"{"name":"demo"}"#).await.unwrap();
        assert_eq!(
            read_json_if_present(&path).await.unwrap(),
            Some(json!({ "name": "demo" }))
        );

        tokio::fs::write(&path, "not json").await.unwrap();
        let error = read_json_if_present(&path).await.unwrap_err();
        assert!(error.to_string().contains("could not parse"));
    }

    #[test]
    fn config_source_reports_scope_path_and_existence() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("buona.json");
        let missing = config_source("selected-target", &path);
        assert_eq!(missing["scope"], "selected-target");
        assert_eq!(missing["exists"], false);

        std::fs::write(&path, "{}").unwrap();
        let present = config_source("selected-target", &path);
        assert_eq!(present["path"], path.to_string_lossy().as_ref());
        assert_eq!(present["exists"], true);
    }
}
