//! Workspace management — creating, listing, deleting workspaces and adding/removing packages.

mod git;
mod ops;
mod types;
mod vscode;

pub(crate) use ops::{add, create, delete, info, list, open, remove_packages, sync};
