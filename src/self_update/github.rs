//! Async GitHub Releases API client.

use anyhow::{Context, Result, bail};
use reqwest::Client;
use std::time::Duration;

use super::types::{GitHubAsset, GitHubRelease};

const REPO: &str = "lpshanley/buona";

fn client() -> Result<Client> {
    Client::builder()
        .user_agent(format!("buona/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create HTTP client")
}

/// Fetch the latest published release.
pub(super) async fn fetch_latest_release() -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client()?
        .get(&url)
        .send()
        .await
        .context("failed to reach GitHub — check your network connection")?;

    if resp.status() == 404 {
        bail!("no releases found for {REPO}");
    }

    resp.error_for_status_ref()
        .with_context(|| format!("GitHub API error: {}", resp.status()))?;

    resp.json::<GitHubRelease>()
        .await
        .context("could not parse GitHub release response")
}

/// Fetch a release by its exact tag name (e.g. "v0.1.5").
pub(super) async fn fetch_release_by_tag(tag: &str) -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/tags/{tag}");
    let resp = client()?
        .get(&url)
        .send()
        .await
        .context("failed to reach GitHub — check your network connection")?;

    if resp.status() == 404 {
        bail!("release \"{tag}\" not found — check the version tag");
    }

    resp.error_for_status_ref()
        .with_context(|| format!("GitHub API error: {}", resp.status()))?;

    resp.json::<GitHubRelease>()
        .await
        .context("could not parse GitHub release response")
}

/// Download a release asset and return its raw bytes.
pub(super) async fn download_asset(url: &str) -> Result<Vec<u8>> {
    let resp = client()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?;

    resp.error_for_status_ref()
        .with_context(|| format!("download failed: {}", resp.status()))?;

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .context("failed to read download body")
}

/// Download and parse a `.sha256` checksum file.
///
/// The file format is: `<hex-digest>  <filename>\n`
pub(super) async fn download_checksum(url: &str) -> Result<String> {
    let body = String::from_utf8(download_asset(url).await?)
        .context("checksum file is not valid UTF-8")?;

    // Parse first field (the hex digest)
    let digest = body
        .split_whitespace()
        .next()
        .context("checksum file is empty")?
        .to_string();

    Ok(digest)
}

/// Find the archive asset matching a target triple.
///
/// Expects naming: `buona-{tag}-{target}.tar.gz`
pub(super) fn find_target_asset<'a>(
    release: &'a GitHubRelease,
    target: &str,
) -> Option<&'a GitHubAsset> {
    let suffix = format!("-{target}.tar.gz");
    release.assets.iter().find(|a| a.name.ends_with(&suffix))
}

/// Find the checksum asset for a target triple.
///
/// Expects naming: `buona-{tag}-{target}.tar.gz.sha256`
pub(super) fn find_checksum_asset<'a>(
    release: &'a GitHubRelease,
    target: &str,
) -> Option<&'a GitHubAsset> {
    let suffix = format!("-{target}.tar.gz.sha256");
    release.assets.iter().find(|a| a.name.ends_with(&suffix))
}
