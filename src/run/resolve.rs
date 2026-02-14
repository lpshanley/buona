//! Pure resolution logic for the `buona run` command.
//!
//! The [`resolve_plan()`] function is the core of the run feature. It takes
//! structured inputs and produces an [`ExecutionPlan`] without performing any
//! filesystem or process operations, making it independently testable.

use std::path::PathBuf;

use super::config::BuonaRunConfig;
use super::detect::detect_build_system;
use super::error::RunError;
use super::systems::{proxy_command, standard_mapping};
use super::types::*;

/// Inputs to plan resolution.
pub(super) struct ResolveInput {
    /// The package directory (cwd for execution).
    pub(super) package_dir: PathBuf,
    /// The command the user typed (first token after `--`).
    pub(super) command: String,
    /// Additional args the user typed (tokens after the command).
    pub(super) extra_args: Vec<String>,
    /// CLI `--system` override (if provided).
    pub(super) cli_system: Option<String>,
    /// Per-package `buona.json` config (if present).
    pub(super) package_config: Option<BuonaRunConfig>,
}

/// Resolve an execution plan from inputs.
///
/// This is the core pure function that drives the entire run feature.
pub(super) fn resolve_plan(input: &ResolveInput) -> Result<ExecutionPlan, RunError> {
    let command_name = &input.command;

    // 1. Check for exec override in buona.json commands
    if let Some(ref config) = input.package_config {
        if let Some(cmd_config) = config.commands.get(command_name) {
            if let Some(ref exec) = cmd_config.exec {
                if exec.is_empty() {
                    return Err(RunError::ConfigError(format!(
                        "\"exec\" for command \"{command_name}\" in buona.json is empty"
                    )));
                }
                let program = exec[0].clone();
                let mut args: Vec<String> = exec[1..].to_vec();
                args.extend(input.extra_args.iter().cloned());

                // Resolve system for display (best-effort)
                let system = resolve_effective_system(
                    cmd_config.system,
                    input.package_config.as_ref(),
                    input.cli_system.as_deref(),
                    &input.package_dir,
                )
                .unwrap_or(BuildSystem::Make); // fallback for display only

                let display = format_display(&program, &args);
                return Ok(ExecutionPlan {
                    cwd: input.package_dir.clone(),
                    system,
                    kind: PlanKind::ExecOverride,
                    program,
                    args,
                    display,
                });
            }
        }
    }

    // 2. Determine effective build system via precedence chain
    let per_command_system = input
        .package_config
        .as_ref()
        .and_then(|c| c.commands.get(command_name))
        .and_then(|c| c.system);

    let system = resolve_effective_system(
        per_command_system,
        input.package_config.as_ref(),
        input.cli_system.as_deref(),
        &input.package_dir,
    )?;

    // 3. Check if command is a standard command
    if let Some(std_cmd) = StandardCommand::parse(command_name) {
        if let Some((program, args)) =
            standard_mapping(system, std_cmd, &input.extra_args, Some(&input.package_dir))
        {
            let display = format_display(&program, &args);
            return Ok(ExecutionPlan {
                cwd: input.package_dir.clone(),
                system,
                kind: PlanKind::Standard,
                program,
                args,
                display,
            });
        }
        // Standard command but no mapping for this system
        return Err(RunError::StandardNotMapped {
            command: command_name.clone(),
            system: system.to_string(),
        });
    }

    // 4. Non-standard command → proxy
    let (program, args) =
        proxy_command(system, command_name, &input.extra_args, Some(&input.package_dir));
    let display = format_display(&program, &args);
    Ok(ExecutionPlan {
        cwd: input.package_dir.clone(),
        system,
        kind: PlanKind::Proxy,
        program,
        args,
        display,
    })
}

/// Resolve the effective build system from the precedence chain:
///
/// 1. Per-command override in buona.json
/// 2. Global system in buona.json (if not "auto")
/// 3. CLI `--system` (if not "auto")
/// 4. Auto-detection from marker files
fn resolve_effective_system(
    per_command: Option<BuildSystem>,
    package_config: Option<&BuonaRunConfig>,
    cli_system: Option<&str>,
    package_dir: &std::path::Path,
) -> Result<BuildSystem, RunError> {
    // 1. Per-command override
    if let Some(system) = per_command {
        return Ok(system);
    }

    // 2. Global system in buona.json (if not "auto")
    if let Some(config) = package_config {
        if config.system != "auto" {
            return parse_system_name(&config.system);
        }
    }

    // 3. CLI --system
    if let Some(name) = cli_system {
        if name != "auto" {
            return parse_system_name(name);
        }
    }

    // 4. Auto-detect
    detect_build_system(package_dir).ok_or_else(|| {
        RunError::NoPackageResolved(format!(
            "could not auto-detect build system in {}\n  \
             Use --system to specify one, or add a buona.json with a \"system\" field.",
            package_dir.display()
        ))
    })
}

/// Parse a system name string into a [`BuildSystem`] enum.
fn parse_system_name(name: &str) -> Result<BuildSystem, RunError> {
    serde_json::from_value(serde_json::Value::String(name.to_string()))
        .map_err(|_| RunError::UnknownSystem(name.to_string()))
}

/// Format a display string from program and args.
fn format_display(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn input_with_system(cmd: &str, system: BuildSystem) -> (TempDir, ResolveInput) {
        let dir = TempDir::new().unwrap();
        // Create marker file for the system
        match system {
            BuildSystem::Cargo => {
                std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
            }
            BuildSystem::Npm => {
                std::fs::write(dir.path().join("package.json"), "{}").unwrap();
            }
            _ => {}
        }
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: cmd.to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        (dir, input)
    }

    // ── standard command resolution ──────────────────────────────

    #[test]
    fn resolves_cargo_test() {
        let (_dir, input) = input_with_system("test", BuildSystem::Cargo);
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.program, "cargo");
        assert_eq!(plan.args, vec!["test"]);
        assert_eq!(plan.kind, PlanKind::Standard);
        assert_eq!(plan.system, BuildSystem::Cargo);
    }

    #[test]
    fn resolves_cargo_test_with_extra_args() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec!["--nocapture".to_string()],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.program, "cargo");
        assert_eq!(plan.args, vec!["test", "--", "--nocapture"]);
    }

    #[test]
    fn resolves_npm_build() {
        let (_dir, input) = input_with_system("build", BuildSystem::Npm);
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.program, "npm");
        assert_eq!(plan.args, vec!["run", "build"]);
        assert_eq!(plan.kind, PlanKind::Standard);
    }

    // ── proxy command resolution ─────────────────────────────────

    #[test]
    fn resolves_proxy_for_unknown_command() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "asm".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.program, "cargo");
        assert_eq!(plan.args, vec!["asm"]);
        assert_eq!(plan.kind, PlanKind::Proxy);
    }

    #[test]
    fn resolves_npm_proxy_with_run() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "my-script".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.program, "npm");
        assert_eq!(plan.args, vec!["run", "my-script"]);
        assert_eq!(plan.kind, PlanKind::Proxy);
    }

    // ── CLI --system override ────────────────────────────────────

    #[test]
    fn cli_system_override() {
        let dir = TempDir::new().unwrap();
        // No marker files — rely on CLI override
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some("cargo".to_string()),
            package_config: None,
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Cargo);
        assert_eq!(plan.program, "cargo");
    }

    #[test]
    fn unknown_cli_system_returns_error() {
        let dir = TempDir::new().unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some("foobar".to_string()),
            package_config: None,
        };
        let result = resolve_plan(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            RunError::UnknownSystem(name) => assert_eq!(name, "foobar"),
            other => panic!("expected UnknownSystem, got: {other}"),
        }
    }

    // ── buona.json config overrides ──────────────────────────────

    #[test]
    fn config_global_system_override() {
        let dir = TempDir::new().unwrap();
        // No marker files — rely on config
        let config = BuonaRunConfig {
            system: "npm".to_string(),
            commands: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Npm);
        assert_eq!(plan.program, "npm");
    }

    #[test]
    fn config_per_command_system_override() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let mut commands = HashMap::new();
        commands.insert(
            "build".to_string(),
            super::super::config::CommandConfig {
                system: Some(BuildSystem::Make),
                exec: None,
            },
        );
        let config = BuonaRunConfig {
            system: "auto".to_string(),
            commands,
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "build".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Make);
        assert_eq!(plan.program, "make");
        assert!(plan.args.is_empty());
    }

    #[test]
    fn config_exec_override() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let mut commands = HashMap::new();
        commands.insert(
            "test".to_string(),
            super::super::config::CommandConfig {
                system: None,
                exec: Some(vec![
                    "pnpm".to_string(),
                    "run".to_string(),
                    "custom-test".to_string(),
                ]),
            },
        );
        let config = BuonaRunConfig {
            system: "auto".to_string(),
            commands,
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.kind, PlanKind::ExecOverride);
        assert_eq!(plan.program, "pnpm");
        assert_eq!(plan.args, vec!["run", "custom-test"]);
    }

    #[test]
    fn empty_exec_override_returns_error() {
        let dir = TempDir::new().unwrap();
        let mut commands = HashMap::new();
        commands.insert(
            "test".to_string(),
            super::super::config::CommandConfig {
                system: None,
                exec: Some(vec![]),
            },
        );
        let config = BuonaRunConfig {
            system: "auto".to_string(),
            commands,
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let result = resolve_plan(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            RunError::ConfigError(msg) => assert!(msg.contains("empty")),
            other => panic!("expected ConfigError, got: {other}"),
        }
    }

    // ── precedence tests ─────────────────────────────────────────

    #[test]
    fn per_command_config_beats_global_config() {
        let dir = TempDir::new().unwrap();
        let mut commands = HashMap::new();
        commands.insert(
            "build".to_string(),
            super::super::config::CommandConfig {
                system: Some(BuildSystem::Make),
                exec: None,
            },
        );
        let config = BuonaRunConfig {
            system: "cargo".to_string(), // global says cargo
            commands,                     // but build says make
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "build".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Make);
    }

    #[test]
    fn global_config_beats_cli_system() {
        let dir = TempDir::new().unwrap();
        let config = BuonaRunConfig {
            system: "npm".to_string(),
            commands: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some("cargo".to_string()), // CLI says cargo
            package_config: Some(config),           // config says npm
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Npm); // config wins
    }

    #[test]
    fn cli_system_beats_auto_detect() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some("npm".to_string()), // CLI says npm
            package_config: None,
        };
        let plan = resolve_plan(&input).unwrap();
        assert_eq!(plan.system, BuildSystem::Npm); // CLI wins over Cargo.toml
    }

    // ── standard not mapped ──────────────────────────────────────

    #[test]
    fn standard_not_mapped_returns_error() {
        let dir = TempDir::new().unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "dev".to_string(),
            extra_args: vec![],
            cli_system: Some("cargo".to_string()),
            package_config: None,
        };
        let result = resolve_plan(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            RunError::StandardNotMapped { command, system } => {
                assert_eq!(command, "dev");
                assert_eq!(system, "cargo");
            }
            other => panic!("expected StandardNotMapped, got: {other}"),
        }
    }

    // ── no system detectable ─────────────────────────────────────

    #[test]
    fn no_system_detectable_returns_error() {
        let dir = TempDir::new().unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let result = resolve_plan(&input);
        assert!(result.is_err());
    }

    // ── display format ───────────────────────────────────────────

    #[test]
    fn display_format() {
        let display = format_display("cargo", &["test".to_string(), "--".to_string(), "--nocapture".to_string()]);
        assert_eq!(display, "cargo test -- --nocapture");
    }
}
