//! Safe discovery command for humans and automation.

use std::env;

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
    let workspace_root = find_optional_workspace_root(&cwd).await?;

    if workspace_root.is_none() && target_name.is_some() {
        return Err(
            RunError::ConfigError("--target requires a buona workspace".to_string()).into(),
        );
    }

    let target = match workspace_root {
        Some(ref root) => {
            let names = target_name.clone().into_iter().collect::<Vec<_>>();
            resolve_targets(&cwd, root, &names, false)
                .await?
                .into_iter()
                .next()
                .context("no execution target resolved")?
        }
        None => resolve_local_target(&cwd).await?,
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

    let document = json!({
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
    });

    if crate::output::is_json() {
        return crate::output::print_json(&document);
    }

    print_text(&document);
    Ok(())
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
