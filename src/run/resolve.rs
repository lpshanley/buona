//! Pure resolution logic for the `buona run` command.
//!
//! The [`resolve_plan()`] function is the core of the run feature. It takes
//! structured inputs and produces an [`ExecutionPlan`] without performing any
//! filesystem or process operations, making it independently testable.

use std::path::PathBuf;

use super::config::{BuonaRunConfig, ConfigSystem};
use super::detect::detect_build_system;
use super::error::RunError;
use super::format::format_display;
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
    pub(super) cli_system: Option<BuildSystem>,
    /// Per-package `buona.json` config (if present).
    pub(super) package_config: Option<BuonaRunConfig>,
}

/// Resolve an execution plan from inputs.
///
/// This is the core pure function that drives the entire run feature.
pub(super) async fn resolve_plan(input: &ResolveInput) -> Result<ExecutionPlan, RunError> {
    let command_name = &input.command;

    // 1. Check for exec override in buona.json commands
    if let Some(ref config) = input.package_config
        && let Some(cmd_config) = config.commands.get(command_name)
        && let Some(ref exec) = cmd_config.exec
    {
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
            input.cli_system,
            &input.package_dir,
        )
        .await
        .ok()
        .flatten();

        let display = format_display(&program, &args);
        return Ok(ExecutionPlan {
            cwd: input.package_dir.clone(),
            system,
            kind: PlanKind::ExecOverride,
            program: Some(program),
            args,
            display,
            skip_reason: None,
        });
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
        input.cli_system,
        &input.package_dir,
    )
    .await?;

    if system.is_none() {
        return Ok(noop_plan(
            input.package_dir.clone(),
            None,
            SkipReason::NoSystemDetected,
        ));
    }
    let system = system.unwrap();

    // 3. Check if command is a standard command
    if let Some(std_cmd) = StandardCommand::parse(command_name) {
        if let Some((program, args)) =
            standard_mapping(system, std_cmd, &input.extra_args, Some(&input.package_dir))
        {
            let display = format_display(&program, &args);
            return Ok(ExecutionPlan {
                cwd: input.package_dir.clone(),
                system: Some(system),
                kind: PlanKind::Standard,
                program: Some(program),
                args,
                display,
                skip_reason: None,
            });
        }
        // Standard command but no mapping for this system
        return Ok(noop_plan(
            input.package_dir.clone(),
            Some(system),
            SkipReason::StandardNotMapped,
        ));
    }

    // 4. Non-standard command → proxy
    let (program, args) = proxy_command(
        system,
        command_name,
        &input.extra_args,
        Some(&input.package_dir),
    );
    let display = format_display(&program, &args);
    Ok(ExecutionPlan {
        cwd: input.package_dir.clone(),
        system: Some(system),
        kind: PlanKind::Proxy,
        program: Some(program),
        args,
        display,
        skip_reason: None,
    })
}

/// Resolve the effective build system from the precedence chain:
///
/// 1. Per-command override in buona.json (most specific intent)
/// 2. CLI `--system` — an explicit flag on this invocation beats the
///    file-level default, matching how every mainstream tool treats
///    flag-vs-config precedence
/// 3. Global system in buona.json (if not "auto")
/// 4. Auto-detection from marker files
async fn resolve_effective_system(
    per_command: Option<BuildSystem>,
    package_config: Option<&BuonaRunConfig>,
    cli_system: Option<BuildSystem>,
    package_dir: &std::path::Path,
) -> Result<Option<BuildSystem>, RunError> {
    // 1. Per-command override
    if let Some(system) = per_command {
        return Ok(Some(system));
    }

    // 2. CLI --system
    if let Some(system) = cli_system {
        return Ok(Some(system));
    }

    // 3. Global system in buona.json
    if let Some(config) = package_config {
        match config.system {
            ConfigSystem::Fixed(system) => return Ok(Some(system)),
            ConfigSystem::Auto => {}
        }
    }

    // 4. Auto-detect
    Ok(detect_build_system(package_dir).await)
}

fn noop_plan(cwd: PathBuf, system: Option<BuildSystem>, reason: SkipReason) -> ExecutionPlan {
    ExecutionPlan {
        cwd,
        system,
        kind: PlanKind::Noop,
        program: None,
        args: vec![],
        display: "skipped".to_string(),
        skip_reason: Some(reason),
    }
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

    #[tokio::test]
    async fn resolves_cargo_test() {
        let (_dir, input) = input_with_system("test", BuildSystem::Cargo);
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.program.as_deref(), Some("cargo"));
        assert_eq!(plan.args, vec!["test"]);
        assert_eq!(plan.kind, PlanKind::Standard);
        assert_eq!(plan.system, Some(BuildSystem::Cargo));
    }

    #[tokio::test]
    async fn resolves_cargo_test_with_extra_args() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec!["--nocapture".to_string()],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.program.as_deref(), Some("cargo"));
        assert_eq!(plan.args, vec!["test", "--", "--nocapture"]);
    }

    #[tokio::test]
    async fn resolves_npm_build() {
        let (_dir, input) = input_with_system("build", BuildSystem::Npm);
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.program.as_deref(), Some("npm"));
        assert_eq!(plan.args, vec!["run", "build"]);
        assert_eq!(plan.kind, PlanKind::Standard);
    }

    // ── proxy command resolution ─────────────────────────────────

    #[tokio::test]
    async fn resolves_proxy_for_unknown_command() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "asm".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.program.as_deref(), Some("cargo"));
        assert_eq!(plan.args, vec!["asm"]);
        assert_eq!(plan.kind, PlanKind::Proxy);
    }

    #[tokio::test]
    async fn resolves_npm_proxy_with_run() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "my-script".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.program.as_deref(), Some("npm"));
        assert_eq!(plan.args, vec!["run", "my-script"]);
        assert_eq!(plan.kind, PlanKind::Proxy);
    }

    // ── CLI --system override ────────────────────────────────────

    #[tokio::test]
    async fn cli_system_override() {
        let dir = TempDir::new().unwrap();
        // No marker files — rely on CLI override
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some(BuildSystem::Cargo),
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Cargo));
        assert_eq!(plan.program.as_deref(), Some("cargo"));
    }

    // ── buona.json config overrides ──────────────────────────────

    #[tokio::test]
    async fn config_global_system_override() {
        let dir = TempDir::new().unwrap();
        // No marker files — rely on config
        let config = BuonaRunConfig {
            system: ConfigSystem::Fixed(BuildSystem::Npm),
            commands: HashMap::new(),
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Npm));
        assert_eq!(plan.program.as_deref(), Some("npm"));
    }

    #[tokio::test]
    async fn config_per_command_system_override() {
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
            system: ConfigSystem::Auto,
            commands,
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "build".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Make));
        assert_eq!(plan.program.as_deref(), Some("make"));
        assert!(plan.args.is_empty());
    }

    #[tokio::test]
    async fn config_exec_override() {
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
            system: ConfigSystem::Auto,
            commands,
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.kind, PlanKind::ExecOverride);
        assert_eq!(plan.program.as_deref(), Some("pnpm"));
        assert_eq!(plan.args, vec!["run", "custom-test"]);
    }

    #[tokio::test]
    async fn empty_exec_override_returns_error() {
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
            system: ConfigSystem::Auto,
            commands,
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let result = resolve_plan(&input).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RunError::ConfigError(msg) => assert!(msg.contains("empty")),
            other => panic!("expected ConfigError, got: {other}"),
        }
    }

    // ── precedence tests ─────────────────────────────────────────

    #[tokio::test]
    async fn per_command_config_beats_global_config() {
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
            system: ConfigSystem::Fixed(BuildSystem::Cargo), // global says cargo
            commands,                                        // but build says make
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "build".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: Some(config),
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Make));
    }

    #[tokio::test]
    async fn cli_system_beats_global_config() {
        let dir = TempDir::new().unwrap();
        let config = BuonaRunConfig {
            system: ConfigSystem::Fixed(BuildSystem::Npm),
            commands: HashMap::new(),
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some(BuildSystem::Cargo), // CLI says cargo
            package_config: Some(config),         // config says npm
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Cargo)); // explicit flag wins
    }

    #[tokio::test]
    async fn per_command_config_beats_cli_system() {
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
            system: ConfigSystem::Auto,
            commands,
            hooks_dir: ".buona/hooks".to_string(),
            hooks: HashMap::new(),
        };
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "build".to_string(),
            extra_args: vec![],
            cli_system: Some(BuildSystem::Cargo), // CLI says cargo
            package_config: Some(config),         // per-command says make
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Make)); // most specific wins
    }

    #[tokio::test]
    async fn cli_system_beats_auto_detect() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: Some(BuildSystem::Npm), // CLI says npm
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.system, Some(BuildSystem::Npm)); // CLI wins over Cargo.toml
    }

    // ── standard not mapped ──────────────────────────────────────

    #[tokio::test]
    async fn standard_not_mapped_returns_noop_plan() {
        let dir = TempDir::new().unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "dev".to_string(),
            extra_args: vec![],
            cli_system: Some(BuildSystem::Cargo),
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.kind, PlanKind::Noop);
        assert_eq!(plan.program, None);
        assert_eq!(plan.skip_reason, Some(SkipReason::StandardNotMapped));
    }

    // ── no system detectable ─────────────────────────────────────

    #[tokio::test]
    async fn no_system_detectable_returns_noop_plan() {
        let dir = TempDir::new().unwrap();
        let input = ResolveInput {
            package_dir: dir.path().to_path_buf(),
            command: "test".to_string(),
            extra_args: vec![],
            cli_system: None,
            package_config: None,
        };
        let plan = resolve_plan(&input).await.unwrap();
        assert_eq!(plan.kind, PlanKind::Noop);
        assert_eq!(plan.program, None);
        assert_eq!(plan.skip_reason, Some(SkipReason::NoSystemDetected));
    }

    // ── display format ───────────────────────────────────────────

    #[test]
    fn display_format() {
        let display = format_display(
            "cargo",
            &[
                "test".to_string(),
                "--".to_string(),
                "--nocapture".to_string(),
            ],
        );
        assert_eq!(display, "cargo test -- --nocapture");
    }
}
