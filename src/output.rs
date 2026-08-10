//! Process-wide output and interaction settings.
//!
//! The CLI configures these settings once, before dispatch. Library-style unit
//! tests that call command functions directly inherit the human-readable
//! defaults.

use std::sync::OnceLock;

use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy)]
struct OutputSettings {
    format: OutputFormat,
    non_interactive: bool,
    colors: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            format: OutputFormat::Text,
            non_interactive: false,
            colors: true,
        }
    }
}

static SETTINGS: OnceLock<OutputSettings> = OnceLock::new();

pub(crate) fn configure(format: OutputFormat, no_color: bool, non_interactive: bool) {
    let colors = !no_color;
    let _ = SETTINGS.set(OutputSettings {
        format,
        // A machine-readable command must never stop for terminal input.
        non_interactive: non_interactive || format == OutputFormat::Json,
        colors,
    });

    if !colors {
        console::set_colors_enabled(false);
        console::set_colors_enabled_stderr(false);
    }
}

fn settings() -> OutputSettings {
    SETTINGS.get().copied().unwrap_or_default()
}

pub(crate) fn is_json() -> bool {
    settings().format == OutputFormat::Json
}

pub(crate) fn is_text() -> bool {
    !is_json()
}

pub(crate) fn is_non_interactive() -> bool {
    settings().non_interactive
}

pub(crate) fn colors_enabled() -> bool {
    settings().colors
}

pub(crate) fn print_json<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Emit a standard mutation result in JSON mode. Text commands render their
/// own richer, human-readable progress and summaries.
pub(crate) fn print_success(operation: &str, data: Value) -> Result<()> {
    if is_json() {
        print_json(&json!({
            "ok": true,
            "operation": operation,
            "data": data,
        }))?;
    }
    Ok(())
}

pub(crate) fn print_error(
    code: &str,
    message: &str,
    exit_code: u8,
    hint: Option<&str>,
    target: Option<&str>,
) {
    let document = json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
            "exit_code": exit_code,
            "hint": hint,
            "target": target,
        }
    });
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&document).expect("error document is serializable")
    );
}

/// Detect `--output json` before clap parsing so usage errors can honor the
/// requested machine-readable format too.
pub(crate) fn json_requested(args: &[std::ffi::OsString]) -> bool {
    args.iter().enumerate().any(|(index, arg)| {
        let value = arg.to_string_lossy();
        value == "--output=json"
            || (value == "--output"
                && args
                    .get(index + 1)
                    .is_some_and(|next| next.to_string_lossy() == "json"))
    })
}

#[macro_export]
macro_rules! textln {
    () => {
        if $crate::output::is_text() {
            println!();
        }
    };
    ($($arg:tt)*) => {
        if $crate::output::is_text() {
            println!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! text_errln {
    ($($arg:tt)*) => {
        if $crate::output::is_text() {
            eprintln!($($arg)*);
        }
    };
}
