//! Workspace management — creating, listing, removing workspaces and adding packages.

mod git;
mod ops;
mod types;
mod vscode;

pub(crate) use ops::{add, create, list, open, remove, sync};
