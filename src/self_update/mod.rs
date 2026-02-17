//! Self-update mechanism — check for and install new versions from GitHub Releases.

mod github;
mod ops;
mod platform;
mod types;

pub(crate) use ops::{UpdateOptions, update};
