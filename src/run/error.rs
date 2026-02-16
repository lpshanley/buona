//! Structured errors for the `buona run` command with specific exit codes.

use std::fmt;
use std::process;

/// Structured errors for the run command.
///
/// Each variant has a specific exit code to allow scripting and CI integration.
#[derive(Debug)]
pub(crate) enum RunError {
    /// Not inside a buona workspace (exit 64).
    NotInWorkspace(String),
    /// Could not determine which package the user is in (exit 65).
    NoPackageResolved(String),
    /// Unknown build system name (exit 66).
    UnknownSystem(String),
    /// buona.json is malformed or has conflicting settings (exit 68).
    ConfigError(String),
    /// Ambiguous hook: multiple files in hooksDir match the same hook name (exit 69).
    AmbiguousHook {
        hook_name: String,
        candidates: Vec<String>,
    },
    /// A hook process failed. Exit code is forwarded from the child process.
    #[allow(dead_code)]
    HookFailed { hook_name: String, exit_code: i32 },
}

impl RunError {
    /// Returns the exit code for this error.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            RunError::NotInWorkspace(_) => 64,
            RunError::NoPackageResolved(_) => 65,
            RunError::UnknownSystem(_) => 66,
            RunError::ConfigError(_) => 68,
            RunError::AmbiguousHook { .. } => 69,
            RunError::HookFailed { exit_code, .. } => *exit_code,
        }
    }

    /// Print the error to stderr and exit with the appropriate code.
    pub(crate) fn exit(&self) -> ! {
        eprintln!("error: {self}");
        process::exit(self.exit_code());
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::NotInWorkspace(msg) => write!(f, "{msg}"),
            RunError::NoPackageResolved(msg) => write!(f, "{msg}"),
            RunError::UnknownSystem(name) => write!(f, "unknown build system: \"{name}\""),
            RunError::ConfigError(msg) => write!(f, "config error: {msg}"),
            RunError::AmbiguousHook {
                hook_name,
                candidates,
            } => {
                write!(
                    f,
                    "ambiguous hook \"{hook_name}\": multiple files match in hooks directory: {}",
                    candidates.join(", ")
                )
            }
            RunError::HookFailed {
                hook_name,
                exit_code,
            } => {
                write!(f, "hook \"{hook_name}\" failed with exit code {exit_code}")
            }
        }
    }
}

impl std::error::Error for RunError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_correct() {
        assert_eq!(RunError::NotInWorkspace(String::new()).exit_code(), 64);
        assert_eq!(RunError::NoPackageResolved(String::new()).exit_code(), 65);
        assert_eq!(RunError::UnknownSystem(String::new()).exit_code(), 66);
        assert_eq!(RunError::ConfigError(String::new()).exit_code(), 68);
    }

    #[test]
    fn display_unknown_system() {
        let err = RunError::UnknownSystem("foobar".to_string());
        assert_eq!(err.to_string(), "unknown build system: \"foobar\"");
    }

    #[test]
    fn display_config_error() {
        let err = RunError::ConfigError("missing field".to_string());
        assert_eq!(err.to_string(), "config error: missing field");
    }

    #[test]
    fn exit_code_ambiguous_hook() {
        assert_eq!(
            RunError::AmbiguousHook {
                hook_name: String::new(),
                candidates: vec![],
            }
            .exit_code(),
            69
        );
    }

    #[test]
    fn exit_code_hook_failed_forwards_child_code() {
        assert_eq!(
            RunError::HookFailed {
                hook_name: String::new(),
                exit_code: 42,
            }
            .exit_code(),
            42
        );
    }

    #[test]
    fn display_ambiguous_hook() {
        let err = RunError::AmbiguousHook {
            hook_name: "prebuild".to_string(),
            candidates: vec!["prebuild.sh".to_string(), "prebuild.py".to_string()],
        };
        assert!(err.to_string().contains("ambiguous"));
        assert!(err.to_string().contains("prebuild.sh, prebuild.py"));
    }

    #[test]
    fn display_hook_failed() {
        let err = RunError::HookFailed {
            hook_name: "prebuild".to_string(),
            exit_code: 1,
        };
        assert_eq!(err.to_string(), "hook \"prebuild\" failed with exit code 1");
    }
}
