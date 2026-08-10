//! Detect command orchestration for `buona detect`.

use std::env;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use crate::styles::Styles;

use super::detect::detect_all_systems;
use super::error::RunError;
use super::ops::{find_optional_workspace_root, require_workspace_for_multi_target};
use super::targets::{resolve_local_target, resolve_targets};

/// Print the auto-detected build system and all marker files found.
pub(super) async fn detect(targets: Vec<String>, recursive: bool) -> Result<()> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    let json_targets = detect_in(&cwd, targets, recursive).await?;

    if crate::output::is_json() {
        crate::output::print_json(&json!({ "targets": json_targets }))?;
    }

    Ok(())
}

async fn detect_in(
    cwd: &Path,
    targets: Vec<String>,
    recursive: bool,
) -> Result<Vec<serde_json::Value>> {
    let s = Styles::default();

    if recursive && !targets.is_empty() {
        return Err(RunError::ConfigError(
            "--recursive cannot be combined with --target/-t".to_string(),
        )
        .into());
    }

    let ws_root = find_optional_workspace_root(cwd).await?;
    require_workspace_for_multi_target(ws_root.is_some(), recursive, &targets)?;

    let detect_targets = match ws_root {
        Some(ref root) => resolve_targets(cwd, root, &targets, recursive).await?,
        None => vec![resolve_local_target(cwd).await?],
    };

    let mut json_targets = Vec::new();
    crate::textln!();
    for target in detect_targets {
        let detections = detect_all_systems(&target.dir).await;
        json_targets.push(json!({
            "target": {
                "name": target.label(),
                "kind": if target.is_workspace_root { "workspace-root" } else { "package" },
                "directory": target.dir,
            },
            "winner": detections.first().map(|detection| detection.system.to_string()),
            "detections": detections.iter().map(|detection| json!({
                "system": detection.system.to_string(),
                "marker": detection.marker,
            })).collect::<Vec<_>>(),
        }));
        crate::textln!(
            "  {} {}",
            s.bold.apply_to("target:"),
            s.cyan.apply_to(target.label())
        );
        if detections.is_empty() {
            crate::textln!("    {} noop", s.dim.apply_to("—"));
            continue;
        }

        let winner = &detections[0];
        crate::textln!(
            "    {} {} (via {})",
            s.green.apply_to("detected:"),
            s.bold.apply_to(winner.system.to_string()),
            s.dim.apply_to(&winner.marker),
        );

        if detections.len() > 1 {
            crate::textln!("    {}", s.dim.apply_to("Other marker files found:"));
            for d in &detections[1..] {
                crate::textln!(
                    "    {}  {} (via {})",
                    s.dim.apply_to("·"),
                    d.system,
                    s.dim.apply_to(&d.marker),
                );
            }
        }
        crate::textln!();
    }

    Ok(json_targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }

    #[tokio::test]
    async fn standalone_detection_returns_winner_and_markers() {
        let targets = detect_in(&fixture("tests/fixtures/systems/cargo"), Vec::new(), false)
            .await
            .unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["target"]["name"], "cargo");
        assert_eq!(targets[0]["winner"], "cargo");
        assert_eq!(targets[0]["detections"][0]["marker"], "Cargo.toml");
    }

    #[tokio::test]
    async fn recursive_workspace_detection_includes_root_and_packages() {
        let targets = detect_in(&fixture("tests/fixtures/workspace"), Vec::new(), true)
            .await
            .unwrap();

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0]["target"]["name"], "root");
        assert_eq!(targets[1]["target"]["name"], "cargo-app");
        assert_eq!(targets[2]["target"]["name"], "node-app");
    }

    #[tokio::test]
    async fn invalid_multi_target_requests_return_configuration_errors() {
        let workspace = fixture("tests/fixtures/workspace");
        let error = detect_in(&workspace, vec!["root".to_string()], true)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cannot be combined"));

        let standalone = fixture("tests/fixtures/systems/cargo");
        let error = detect_in(&standalone, vec!["root".to_string()], false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("require a buona workspace"));
    }
}
