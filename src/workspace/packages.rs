//! Package discovery helpers shared by workspace and run flows.

use std::path::Path;

use anyhow::{Context, Result};

/// Scan the `src/` directory of a workspace and return sorted package names.
///
/// Each subdirectory of `src/` is treated as a package. Returns an empty vec
/// when `src/` does not exist.
pub(crate) async fn list_package_names(ws_root: &Path) -> Result<Vec<String>> {
    let src_dir = ws_root.join("src");
    if !tokio::fs::try_exists(&src_dir).await.unwrap_or(false) {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(&src_dir)
        .await
        .with_context(|| format!("could not read src directory: {}", src_dir.display()))?;

    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }

    names.sort();
    Ok(names)
}
