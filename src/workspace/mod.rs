//! Workspace management — creating, listing, deleting workspaces and adding/removing packages.

mod add_packages;
mod adopt_package;
mod git;
mod git_ops;
mod info;
mod locator;
mod open_workspace;
mod ops;
mod packages;
mod remove_packages;
mod sync_packages;
mod template;
mod types;
mod vscode;
mod workspace_file;

pub(crate) use locator::find_workspace_root;
pub(crate) use ops::{
    CreateOptions, add, adopt, config_get, config_set, config_unset, create, delete, info, list,
    open, remove_packages, sync,
};
pub(crate) use packages::list_package_names;
