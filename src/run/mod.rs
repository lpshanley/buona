//! Build system runner — detects, resolves, and executes build commands.

mod config;
mod detect;
mod error;
mod ops;
mod resolve;
mod systems;
mod types;

pub(crate) use error::RunError;
pub(crate) use ops::{RunOptions, detect, execute};
