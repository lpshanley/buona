//! Detect command orchestration for `buona detect`.

use std::env;

use anyhow::{Context, Result};

use crate::styles::Styles;
use crate::workspace;

use super::detect::detect_all_systems;
use super::error::RunError;
use super::targets::resolve_targets;

/// Print the auto-detected build system and all marker files found.
pub(super) async fn detect(targets: Vec<String>, recursive: bool) -> Result<()> {
    let s = Styles::default();

    if recursive && !targets.is_empty() {
        return Err(RunError::ConfigError(
            "--recursive cannot be combined with --target/-t".to_string(),
        )
        .into());
    }

    let cwd = env::current_dir().context("could not determine current directory")?;
    let ws_root = workspace::find_workspace_root(&cwd).await.map_err(|_| {
        RunError::NotInWorkspace(
            "not inside a buona workspace (no buona.workspace.json found)\n  \
             Run this command from within a workspace."
                .to_string(),
        )
    })?;

    let detect_targets = resolve_targets(&cwd, &ws_root, &targets, recursive).await?;

    println!();
    for target in detect_targets {
        let detections = detect_all_systems(&target.dir).await;
        println!(
            "  {} {}",
            s.bold.apply_to("target:"),
            s.cyan.apply_to(target.label())
        );
        if detections.is_empty() {
            println!("    {} noop", s.dim.apply_to("—"));
            continue;
        }

        let winner = &detections[0];
        println!(
            "    {} {} (via {})",
            s.green.apply_to("detected:"),
            s.bold.apply_to(winner.system.to_string()),
            s.dim.apply_to(&winner.marker),
        );

        if detections.len() > 1 {
            println!("    {}", s.dim.apply_to("Other marker files found:"));
            for d in &detections[1..] {
                println!(
                    "    {}  {} (via {})",
                    s.dim.apply_to("·"),
                    d.system,
                    s.dim.apply_to(&d.marker),
                );
            }
        }
        println!();
    }

    Ok(())
}
