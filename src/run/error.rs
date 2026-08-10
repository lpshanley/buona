//! Structured errors for the `buona run` command with specific exit codes.

use std::fmt;

/// Structured errors for the run command.
///
/// Each variant has a specific exit code to allow scripting and CI integration.
#[derive(Debug)]
pub(crate) enum RunError {
    /// Could not determine which package the user is in (exit 65).
    NoPackageResolved(String),
    /// buona.json is malformed or has conflicting settings (exit 68).
    ConfigError(String),
    /// Ambiguous hook: multiple files in hooksDir match the same hook name (exit 69).
    AmbiguousHook {
        hook_name: String,
        candidates: Vec<String>,
    },
    /// A resolved main command failed. Exit code is forwarded from the child process.
    CommandFailed { command: String, exit_code: i32 },
    /// A hook process failed. Exit code is forwarded from the child process.
    HookFailed { hook_name: String, exit_code: i32 },
}

impl RunError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            RunError::NoPackageResolved(_) => "target-resolution",
            RunError::ConfigError(_) => "configuration",
            RunError::AmbiguousHook { .. } => "ambiguous-hook",
            RunError::CommandFailed { .. } => "command-failed",
            RunError::HookFailed { .. } => "hook-failed",
        }
    }

    pub(crate) fn hint(&self) -> Option<&'static str> {
        match self {
            RunError::NoPackageResolved(_) => {
                Some("Run from a package directory or select a target explicitly.")
            }
            RunError::ConfigError(_) => Some("Check buona.json and the command arguments."),
            RunError::AmbiguousHook { .. } => {
                Some("Keep only one matching hook file or configure the hook explicitly.")
            }
            RunError::CommandFailed { .. } | RunError::HookFailed { .. } => None,
        }
    }

    /// Returns the exit code for this error.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            RunError::NoPackageResolved(_) => 65,
            RunError::ConfigError(_) => 68,
            RunError::AmbiguousHook { .. } => 69,
            RunError::CommandFailed { exit_code, .. } => *exit_code,
            RunError::HookFailed { exit_code, .. } => *exit_code,
        }
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::NoPackageResolved(msg) => write!(f, "{msg}"),
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
            RunError::CommandFailed { command, exit_code } => {
                write!(f, "command \"{command}\" failed with exit code {exit_code}")
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
        assert_eq!(RunError::NoPackageResolved(String::new()).exit_code(), 65);
        assert_eq!(RunError::ConfigError(String::new()).exit_code(), 68);
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
    fn exit_code_command_failed_forwards_child_code() {
        assert_eq!(
            RunError::CommandFailed {
                command: "cargo test".to_string(),
                exit_code: 5,
            }
            .exit_code(),
            5
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

    #[test]
    fn display_command_failed() {
        let err = RunError::CommandFailed {
            command: "cargo test".to_string(),
            exit_code: 101,
        };
        assert_eq!(
            err.to_string(),
            "command \"cargo test\" failed with exit code 101"
        );
    }

    #[test]
    fn structured_codes_and_hints_cover_every_variant() {
        let cases = [
            (
                RunError::NoPackageResolved("missing target".to_string()),
                "target-resolution",
                Some("Run from a package directory or select a target explicitly."),
            ),
            (
                RunError::ConfigError("invalid config".to_string()),
                "configuration",
                Some("Check buona.json and the command arguments."),
            ),
            (
                RunError::AmbiguousHook {
                    hook_name: "pretest".to_string(),
                    candidates: vec!["pretest.sh".to_string(), "pretest.py".to_string()],
                },
                "ambiguous-hook",
                Some("Keep only one matching hook file or configure the hook explicitly."),
            ),
            (
                RunError::CommandFailed {
                    command: "cargo test".to_string(),
                    exit_code: 1,
                },
                "command-failed",
                None,
            ),
            (
                RunError::HookFailed {
                    hook_name: "pretest".to_string(),
                    exit_code: 1,
                },
                "hook-failed",
                None,
            ),
        ];

        for (error, code, hint) in cases {
            assert_eq!(error.code(), code);
            assert_eq!(error.hint(), hint);
        }
    }
}
