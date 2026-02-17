//! Self-update orchestration — check, download, verify, and replace.

use std::io::Read;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::styles::Styles;

use super::github;
use super::platform;

/// Options for the self-update command.
pub(crate) struct UpdateOptions {
    /// Only check for updates, don't install.
    pub(crate) check: bool,
    /// Skip confirmation prompt.
    pub(crate) yes: bool,
    /// Specific version to install (e.g. "v0.1.5" or "0.1.5").
    pub(crate) version: Option<String>,
}

/// Run the self-update flow.
pub(crate) async fn update(options: UpdateOptions) -> Result<()> {
    let s = Styles::default();
    let current_version = env!("CARGO_PKG_VERSION");
    let target = platform::current_target();

    println!();
    println!("  {} buona self update", s.bold.apply_to("⚡"),);
    println!("  {}", s.dim.apply_to("───────────────────────────"));
    println!(
        "  {}  v{}",
        s.dim.apply_to("Current version:"),
        current_version,
    );
    println!("  {}  {}", s.dim.apply_to("Platform:"), target,);
    println!();

    // Determine which release to fetch
    let tag = options.version.as_ref().map(|v| normalize_tag(v));

    let release = match &tag {
        Some(t) => {
            println!(
                "  {} Fetching release {} ...",
                s.dim.apply_to("→"),
                s.cyan.apply_to(t),
            );
            github::fetch_release_by_tag(t).await?
        }
        None => {
            println!("  {} Checking for latest release ...", s.dim.apply_to("→"),);
            github::fetch_latest_release().await?
        }
    };

    let release_version = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);

    // Compare versions
    if tag.is_none() && release_version == current_version {
        println!(
            "  {} Already up to date (v{})",
            s.green.apply_to("✔"),
            current_version,
        );
        println!();
        return Ok(());
    }

    println!(
        "  {}  {}",
        s.dim.apply_to("Available version:"),
        s.bold.apply_to(format!("v{release_version}")),
    );

    // Find matching assets
    let archive_asset = github::find_target_asset(&release, target).with_context(|| {
        let available: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
        format!(
            "no prebuilt binary for platform \"{target}\".\n  Available assets: {}",
            available.join(", ")
        )
    })?;

    let checksum_asset = github::find_checksum_asset(&release, target);

    if options.check {
        if release_version == current_version {
            println!("  {} Already up to date", s.green.apply_to("✔"),);
        } else {
            println!(
                "  {} Update available: v{} → v{}",
                s.cyan.apply_to("→"),
                current_version,
                release_version,
            );
        }
        println!();
        return Ok(());
    }

    // Confirm unless --yes or explicit version
    if !options.yes && tag.is_none() {
        println!();
        let confirmed = dialoguer::Confirm::new()
            .with_prompt(format!("  Install v{release_version}?"))
            .default(true)
            .interact()
            .context("failed to read input")?;

        if !confirmed {
            println!("  Aborted.");
            println!();
            return Ok(());
        }
    }

    // Download archive
    println!();
    println!(
        "  {} Downloading {} ...",
        s.dim.apply_to("→"),
        s.cyan.apply_to(&archive_asset.name),
    );

    let archive_bytes = github::download_asset(&archive_asset.browser_download_url).await?;

    // Verify checksum
    if let Some(cs_asset) = checksum_asset {
        println!("  {} Verifying checksum ...", s.dim.apply_to("→"),);

        let expected = github::download_checksum(&cs_asset.browser_download_url).await?;
        let actual = sha256_hex(&archive_bytes);

        if actual != expected {
            bail!(
                "checksum mismatch!\n  \
                 expected: {expected}\n  \
                 actual:   {actual}\n  \
                 The downloaded file may be corrupted."
            );
        }

        println!("  {} Checksum verified", s.green.apply_to("✔"),);
    }

    // Extract binary from tar.gz
    println!("  {} Extracting binary ...", s.dim.apply_to("→"),);

    let binary_data = extract_binary_from_tar_gz(&archive_bytes)?;

    // Replace current binary
    let current_exe =
        std::env::current_exe().context("could not determine current executable path")?;
    let current_exe = current_exe.canonicalize().unwrap_or(current_exe);

    println!(
        "  {} Installing to {} ...",
        s.dim.apply_to("→"),
        s.dim.apply_to(current_exe.display().to_string()),
    );

    replace_binary(&current_exe, &binary_data)?;

    println!();
    println!(
        "  {} Updated to v{}",
        s.green.apply_to("✔"),
        s.bold.apply_to(release_version),
    );
    println!();

    Ok(())
}

/// Normalize a version string into a git tag.
///
/// "0.1.5" → "v0.1.5", "v0.1.5" → "v0.1.5"
fn normalize_tag(version: &str) -> String {
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

/// Compute SHA-256 hex digest of some bytes.
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Extract the `buona` binary from a `.tar.gz` archive in memory.
fn extract_binary_from_tar_gz(archive_bytes: &[u8]) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().context("could not read tar archive")? {
        let mut entry = entry.context("could not read tar entry")?;
        let path = entry.path().context("could not read entry path")?;

        if path.file_name().and_then(|n| n.to_str()) == Some("buona") {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .context("could not read binary from archive")?;
            return Ok(buf);
        }
    }

    bail!("archive does not contain a \"buona\" binary")
}

/// Atomically replace the current binary with new data.
///
/// Strategy: write to a temp file in the same directory, then rename.
/// This ensures the replacement is atomic on the same filesystem.
fn replace_binary(target: &std::path::Path, data: &[u8]) -> Result<()> {
    let dir = target
        .parent()
        .context("could not determine binary directory")?;

    let tmp_path = dir.join(".buona.update.tmp");

    // Write new binary
    std::fs::write(&tmp_path, data)
        .with_context(|| format!("could not write temp file: {}", tmp_path.display()))?;

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("could not set permissions on {}", tmp_path.display()))?;
    }

    // Atomic rename
    std::fs::rename(&tmp_path, target).with_context(|| {
        format!(
            "could not replace binary at {}\n  \
             You may need elevated privileges (e.g. sudo).",
            target.display()
        )
    })?;

    Ok(())
}
