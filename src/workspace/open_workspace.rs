//! Editor-open workflow for workspaces.

use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use crate::config::BuonaConfig;
use crate::styles::Styles;

use super::types::read_meta;
use super::workspace_file::sync_workspace_file;

/// Open a workspace at a specific root in the configured editor.
///
/// Internal helper that opens the workspace without resolving by name.
pub(super) async fn open_workspace_at(ws_root: &Path, cfg: &BuonaConfig) -> Result<()> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let ws_file_path = sync_workspace_file(ws_root, &meta).await?;

    let ide_cmd = cfg.ide.command();

    println!(
        "  {} Opening in {} ...",
        s.dim.apply_to("→"),
        s.bold.apply_to(cfg.ide.to_string())
    );

    let status = Command::new(ide_cmd)
        .arg(&ws_file_path)
        .status()
        .await
        .with_context(|| {
            format!(
                "failed to launch {ide_cmd} — is {} installed and on your PATH?",
                cfg.ide
            )
        })?;

    if !status.success() {
        bail!("{ide_cmd} exited with {status}");
    }

    println!(
        "  {} Opened {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(
            ws_file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    );
    println!();

    Ok(())
}
