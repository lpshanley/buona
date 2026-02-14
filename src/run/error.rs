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
    /// Standard command has no mapping for this system (exit 67).
    StandardNotMapped { command: String, system: String },
    /// buona.json is malformed or has conflicting settings (exit 68).
    ConfigError(String),
}

impl RunError {
    /// Returns the exit code for this error.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            RunError::NotInWorkspace(_) => 64,
            RunError::NoPackageResolved(_) => 65,
            RunError::UnknownSystem(_) => 66,
            RunError::StandardNotMapped { .. } => 67,
            RunError::ConfigError(_) => 68,
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
            RunError::StandardNotMapped { command, system } => {
                write!(
                    f,
                    "command \"{command}\" has no mapping for system \"{system}\""
                )
            }
            RunError::ConfigError(msg) => write!(f, "config error: {msg}"),
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
        assert_eq!(
            RunError::StandardNotMapped {
                command: String::new(),
                system: String::new()
            }
            .exit_code(),
            67
        );
        assert_eq!(RunError::ConfigError(String::new()).exit_code(), 68);
    }

    #[test]
    fn display_unknown_system() {
        let err = RunError::UnknownSystem("foobar".to_string());
        assert_eq!(err.to_string(), "unknown build system: \"foobar\"");
    }

    #[test]
    fn display_standard_not_mapped() {
        let err = RunError::StandardNotMapped {
            command: "bench".to_string(),
            system: "make".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "command \"bench\" has no mapping for system \"make\""
        );
    }

    #[test]
    fn display_config_error() {
        let err = RunError::ConfigError("missing field".to_string());
        assert_eq!(err.to_string(), "config error: missing field");
    }
}
