//! Compile-time platform detection via the TARGET env var from build.rs.

/// Returns the Rust target triple this binary was compiled for.
///
/// This value is baked in at compile time by `build.rs`.
pub(super) fn current_target() -> &'static str {
    env!("TARGET")
}
