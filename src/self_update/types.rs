//! GitHub Release API response types.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct GitHubRelease {
    pub(super) tag_name: String,
    pub(super) assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitHubAsset {
    pub(super) name: String,
    pub(super) browser_download_url: String,
}
