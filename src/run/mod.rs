//! Build system runner — detects, resolves, and executes build commands.

mod config;
mod detect;
mod detect_cmd;
mod error;
mod executor;
mod format;
mod hooks;
mod init;
mod inspect;
mod ops;
mod output;
mod planner;
mod resolve;
mod systems;
mod targets;
mod types;

pub(crate) use error::RunError;
pub(crate) use init::{InitOptions, init};
pub(crate) use inspect::inspect;
pub(crate) use ops::{RunOptions, detect, execute};
pub(crate) use types::{BuildSystem, FailPolicy};
