//! Core data types for the `buona run` command.

use std::fmt;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Known build systems that buona can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BuildSystem {
    Cargo,
    Go,
    Npm,
    Pnpm,
    Yarn,
    Bun,
    Uv,
    Poetry,
    Make,
    Just,
    Gradle,
    Maven,
}

/// Failure behavior for parallel runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FailPolicy {
    FailFast,
    Continue,
}

impl fmt::Display for FailPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailPolicy::FailFast => write!(f, "fail-fast"),
            FailPolicy::Continue => write!(f, "continue"),
        }
    }
}

#[cfg(test)]
impl BuildSystem {
    const ALL: &[BuildSystem] = &[
        BuildSystem::Cargo,
        BuildSystem::Go,
        BuildSystem::Npm,
        BuildSystem::Pnpm,
        BuildSystem::Yarn,
        BuildSystem::Bun,
        BuildSystem::Uv,
        BuildSystem::Poetry,
        BuildSystem::Make,
        BuildSystem::Just,
        BuildSystem::Gradle,
        BuildSystem::Maven,
    ];
}

impl fmt::Display for BuildSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildSystem::Cargo => write!(f, "cargo"),
            BuildSystem::Go => write!(f, "go"),
            BuildSystem::Npm => write!(f, "npm"),
            BuildSystem::Pnpm => write!(f, "pnpm"),
            BuildSystem::Yarn => write!(f, "yarn"),
            BuildSystem::Bun => write!(f, "bun"),
            BuildSystem::Uv => write!(f, "uv"),
            BuildSystem::Poetry => write!(f, "poetry"),
            BuildSystem::Make => write!(f, "make"),
            BuildSystem::Just => write!(f, "just"),
            BuildSystem::Gradle => write!(f, "gradle"),
            BuildSystem::Maven => write!(f, "maven"),
        }
    }
}

/// Standard commands that buona recognizes and maps to system-specific invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StandardCommand {
    Install,
    Build,
    Run,
    Test,
    Lint,
    Fmt,
    Clean,
    Publish,
    Bench,
    Doc,
    Dev,
}

impl StandardCommand {
    /// Try to parse a string into a standard command.
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "install" => Some(Self::Install),
            "build" => Some(Self::Build),
            "run" => Some(Self::Run),
            "test" => Some(Self::Test),
            "lint" => Some(Self::Lint),
            "fmt" | "format" => Some(Self::Fmt),
            "clean" => Some(Self::Clean),
            "publish" => Some(Self::Publish),
            "bench" => Some(Self::Bench),
            "doc" | "docs" => Some(Self::Doc),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }
}

/// Describes how the execution plan was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanKind {
    /// A standard command with a known system mapping (e.g., "test" → "cargo test").
    Standard,
    /// An unrecognized command proxied through the build system (e.g., "my-script" → "npm run my-script").
    Proxy,
    /// An explicit exec override from buona.json (e.g., "test" → ["pnpm", "run", "custom-test"]).
    ExecOverride,
    /// No runnable command was resolved for this target.
    Noop,
}

/// Why a command stage was intentionally skipped/nooped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// No build system could be resolved via config/CLI/detection.
    NoSystemDetected,
    /// Command is standard, but the resolved system has no mapping for it.
    StandardNotMapped,
}

impl SkipReason {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            SkipReason::NoSystemDetected => "no-system",
            SkipReason::StandardNotMapped => "unmapped",
        }
    }
}

/// The phase at which a hook runs relative to the main command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookPhase {
    Pre,
    Post,
}

impl fmt::Display for HookPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookPhase::Pre => write!(f, "pre"),
            HookPhase::Post => write!(f, "post"),
        }
    }
}

/// How a hook was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookSource {
    /// Defined explicitly in the `hooks` map of buona.json.
    Explicit,
    /// Discovered as a file in `hooksDir`.
    Convention,
}

/// A fully resolved hook, ready to execute.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ResolvedHook {
    /// Pre or post.
    pub(crate) phase: HookPhase,
    /// The hook name (e.g., "prebuild", "posttest").
    pub(crate) name: String,
    /// How this hook was found.
    pub(crate) source: HookSource,
    /// The program to execute.
    pub(crate) program: String,
    /// Arguments to the program.
    pub(crate) args: Vec<String>,
    /// Working directory.
    pub(crate) cwd: PathBuf,
    /// Human-readable display string.
    pub(crate) display: String,
}

/// The fully resolved execution plan. Contains everything needed to spawn the process.
#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    /// Working directory for the command.
    pub(crate) cwd: PathBuf,
    /// The resolved build system, when one exists.
    pub(crate) system: Option<BuildSystem>,
    /// How this plan was derived.
    pub(crate) kind: PlanKind,
    /// The program to execute (e.g., "cargo", "npm") when runnable.
    pub(crate) program: Option<String>,
    /// Arguments to pass to the program.
    pub(crate) args: Vec<String>,
    /// Human-readable display string for output.
    pub(crate) display: String,
    /// Why this plan is skipped/noop, if applicable.
    pub(crate) skip_reason: Option<SkipReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_system_display() {
        assert_eq!(BuildSystem::Cargo.to_string(), "cargo");
        assert_eq!(BuildSystem::Npm.to_string(), "npm");
        assert_eq!(BuildSystem::Gradle.to_string(), "gradle");
        assert_eq!(BuildSystem::Maven.to_string(), "maven");
    }

    #[test]
    fn build_system_serde_round_trips() {
        for &system in BuildSystem::ALL {
            let json = serde_json::to_string(&system).unwrap();
            let deserialized: BuildSystem = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, system);
        }
    }

    #[test]
    fn build_system_serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&BuildSystem::Cargo).unwrap(),
            "\"cargo\""
        );
        assert_eq!(
            serde_json::to_string(&BuildSystem::Gradle).unwrap(),
            "\"gradle\""
        );
    }

    #[test]
    fn standard_command_parse_known() {
        assert_eq!(
            StandardCommand::parse("install"),
            Some(StandardCommand::Install)
        );
        assert_eq!(
            StandardCommand::parse("build"),
            Some(StandardCommand::Build)
        );
        assert_eq!(StandardCommand::parse("run"), Some(StandardCommand::Run));
        assert_eq!(StandardCommand::parse("test"), Some(StandardCommand::Test));
        assert_eq!(StandardCommand::parse("lint"), Some(StandardCommand::Lint));
        assert_eq!(StandardCommand::parse("fmt"), Some(StandardCommand::Fmt));
        assert_eq!(StandardCommand::parse("format"), Some(StandardCommand::Fmt));
        assert_eq!(
            StandardCommand::parse("clean"),
            Some(StandardCommand::Clean)
        );
        assert_eq!(
            StandardCommand::parse("publish"),
            Some(StandardCommand::Publish)
        );
        assert_eq!(
            StandardCommand::parse("bench"),
            Some(StandardCommand::Bench)
        );
        assert_eq!(StandardCommand::parse("doc"), Some(StandardCommand::Doc));
        assert_eq!(StandardCommand::parse("docs"), Some(StandardCommand::Doc));
        assert_eq!(StandardCommand::parse("dev"), Some(StandardCommand::Dev));
    }

    #[test]
    fn standard_command_parse_unknown() {
        assert_eq!(StandardCommand::parse("my-script"), None);
        assert_eq!(StandardCommand::parse("custom"), None);
        assert_eq!(StandardCommand::parse(""), None);
    }

    #[test]
    fn hook_phase_display() {
        assert_eq!(HookPhase::Pre.to_string(), "pre");
        assert_eq!(HookPhase::Post.to_string(), "post");
    }

    #[test]
    fn fail_policy_display() {
        assert_eq!(FailPolicy::FailFast.to_string(), "fail-fast");
        assert_eq!(FailPolicy::Continue.to_string(), "continue");
    }
}
