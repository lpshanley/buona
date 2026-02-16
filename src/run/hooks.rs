//! Hook resolution for the `buona run` command.
//!
//! Hooks are `pre<command>` and `post<command>` scripts that run around a
//! standard command. They are resolved from two sources in priority order:
//!
//! 1. Explicit `hooks` map in `buona.json` (highest priority)
//! 2. Convention-based files discovered in `hooksDir`
//!
//! The resolution logic is split into a pure layer (testable without I/O) and
//! a filesystem layer (for scanning the hooks directory).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::config::HookValue;
use super::error::RunError;
use super::systems::{proxy_command, standard_mapping};
use super::types::*;

// ── Filesystem layer ────────────────────────────────────────────────

/// A hook file discovered in the hooks directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HookFile {
    /// The hook name (filename stem, without extension), e.g. "prebuild".
    pub(super) name: String,
    /// Full path to the file.
    pub(super) path: PathBuf,
    /// Whether the file has the executable permission bit set.
    pub(super) executable: bool,
}

/// Check if a file has any executable permission bit set.
#[cfg(unix)]
fn is_file_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        metadata.permissions().mode() & 0o111 != 0
    } else {
        false
    }
}

#[cfg(not(unix))]
fn is_file_executable(_path: &Path) -> bool {
    true
}

/// Scan `hooks_dir` for files that could be hook scripts.
///
/// Returns all discovered hook files. The caller is responsible for filtering
/// by name and checking for ambiguity. Returns an empty vec if the directory
/// does not exist.
pub(super) fn scan_hooks_dir(hooks_dir: &Path) -> Vec<HookFile> {
    let entries = match std::fs::read_dir(hooks_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stem = match path.file_stem() {
            Some(s) => s.to_string_lossy().to_string(),
            None => continue,
        };
        let executable = is_file_executable(&path);
        results.push(HookFile {
            name: stem,
            path,
            executable,
        });
    }
    results
}

// ── Pure resolution layer ───────────────────────────────────────────

/// Inputs for hook resolution.
pub(super) struct HookResolveInput {
    /// The standard command name (e.g. "build", "test").
    pub(super) command: String,
    /// The package directory (cwd for hook execution).
    pub(super) package_dir: PathBuf,
    /// Explicit hooks from `buona.json` (if present).
    pub(super) explicit_hooks: HashMap<String, HookValue>,
    /// Hook files discovered from `hooksDir` scanning.
    pub(super) convention_hooks: Vec<HookFile>,
}

/// A warning produced during hook resolution (e.g. non-executable file).
#[derive(Debug, Clone)]
pub(super) struct HookWarning {
    pub(super) hook_name: String,
    pub(super) message: String,
}

/// Result of hook resolution: resolved hooks plus any warnings.
#[derive(Debug, Clone)]
pub(super) struct HookResolution {
    pub(super) pre_hook: Option<ResolvedHook>,
    pub(super) post_hook: Option<ResolvedHook>,
    pub(super) warnings: Vec<HookWarning>,
}

/// Resolve hooks for a given command.
///
/// This is the main entry point. It resolves pre and post hooks independently,
/// collecting any warnings along the way.
pub(super) fn resolve_hooks(input: &HookResolveInput) -> Result<HookResolution, RunError> {
    let pre_name = format!("pre{}", input.command);
    let post_name = format!("post{}", input.command);

    let mut warnings = Vec::new();

    let pre_hook = resolve_single_hook(
        &pre_name,
        HookPhase::Pre,
        &input.package_dir,
        &input.explicit_hooks,
        &input.convention_hooks,
        &mut warnings,
    )?;

    let post_hook = resolve_single_hook(
        &post_name,
        HookPhase::Post,
        &input.package_dir,
        &input.explicit_hooks,
        &input.convention_hooks,
        &mut warnings,
    )?;

    Ok(HookResolution {
        pre_hook,
        post_hook,
        warnings,
    })
}

fn resolve_single_hook(
    hook_name: &str,
    phase: HookPhase,
    package_dir: &Path,
    explicit_hooks: &HashMap<String, HookValue>,
    convention_hooks: &[HookFile],
    warnings: &mut Vec<HookWarning>,
) -> Result<Option<ResolvedHook>, RunError> {
    // 1. Check explicit hooks map (highest priority)
    if let Some(value) = explicit_hooks.get(hook_name) {
        return resolve_hook_value(hook_name, phase, value, package_dir, HookSource::Explicit)
            .map(Some);
    }

    // 2. Check convention-based hooks
    let matches: Vec<&HookFile> = convention_hooks
        .iter()
        .filter(|f| f.name == hook_name)
        .collect();

    match matches.len() {
        0 => Ok(None),
        1 => {
            let hook_file = &matches[0];
            if !hook_file.executable {
                warnings.push(HookWarning {
                    hook_name: hook_name.to_string(),
                    message: format!(
                        "file {} is not executable, skipping",
                        hook_file.path.display()
                    ),
                });
                return Ok(None);
            }
            let path_str = hook_file.path.to_string_lossy().to_string();
            Ok(Some(ResolvedHook {
                phase,
                name: hook_name.to_string(),
                source: HookSource::Convention,
                program: path_str.clone(),
                args: vec![],
                cwd: package_dir.to_path_buf(),
                display: path_str,
            }))
        }
        _ => {
            let candidates = matches
                .iter()
                .map(|f| {
                    f.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                })
                .collect();
            Err(RunError::AmbiguousHook {
                hook_name: hook_name.to_string(),
                candidates,
            })
        }
    }
}

/// Interpret an explicit hook value.
///
/// - String values: if recognized as a build system name, use that system's
///   template for the command derived from the hook name. Otherwise treat as a
///   verbatim shell command and run via `sh -c`.
/// - Array values: execute directly as argv (`[program, arg1, ...]`).
fn resolve_hook_value(
    hook_name: &str,
    phase: HookPhase,
    value: &HookValue,
    package_dir: &Path,
    source: HookSource,
) -> Result<ResolvedHook, RunError> {
    match value {
        HookValue::Argv(argv) => {
            if argv.is_empty() {
                return Err(RunError::ConfigError(format!(
                    "hook \"{hook_name}\" has empty argv; expected at least a program"
                )));
            }

            let program = argv[0].clone();
            let args = argv[1..].to_vec();
            let display = format_display(&program, &args);
            Ok(ResolvedHook {
                phase,
                name: hook_name.to_string(),
                source,
                program,
                args,
                cwd: package_dir.to_path_buf(),
                display,
            })
        }
        HookValue::Script(script) => {
            // Try parsing as a build system
            if let Ok(system) = parse_as_build_system(script) {
                let command = strip_hook_prefix(hook_name);

                // Try standard mapping first
                if let Some(std_cmd) = StandardCommand::parse(&command) {
                    if let Some((program, args)) =
                        standard_mapping(system, std_cmd, &[], Some(package_dir))
                    {
                        let display = format_display(&program, &args);
                        return Ok(ResolvedHook {
                            phase,
                            name: hook_name.to_string(),
                            source,
                            program,
                            args,
                            cwd: package_dir.to_path_buf(),
                            display,
                        });
                    }
                }

                // Fall back to proxy command
                let (program, args) = proxy_command(system, &command, &[], Some(package_dir));
                let display = format_display(&program, &args);
                return Ok(ResolvedHook {
                    phase,
                    name: hook_name.to_string(),
                    source,
                    program,
                    args,
                    cwd: package_dir.to_path_buf(),
                    display,
                });
            }

            // Not a system name — treat as verbatim shell command
            Ok(ResolvedHook {
                phase,
                name: hook_name.to_string(),
                source,
                program: "sh".to_string(),
                args: vec!["-c".to_string(), script.to_string()],
                cwd: package_dir.to_path_buf(),
                display: script.to_string(),
            })
        }
    }
}

/// Strip the "pre" or "post" prefix from a hook name to get the command name.
fn strip_hook_prefix(hook_name: &str) -> String {
    if let Some(rest) = hook_name.strip_prefix("pre") {
        rest.to_string()
    } else if let Some(rest) = hook_name.strip_prefix("post") {
        rest.to_string()
    } else {
        hook_name.to_string()
    }
}

/// Try to parse a string as a [`BuildSystem`].
fn parse_as_build_system(name: &str) -> Result<BuildSystem, ()> {
    serde_json::from_value::<BuildSystem>(serde_json::Value::String(name.to_string()))
        .map_err(|_| ())
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
    use tempfile::TempDir;

    // ── strip_hook_prefix ───────────────────────────────────────────

    #[test]
    fn strip_pre_prefix() {
        assert_eq!(strip_hook_prefix("prebuild"), "build");
        assert_eq!(strip_hook_prefix("pretest"), "test");
        assert_eq!(strip_hook_prefix("prelint"), "lint");
    }

    #[test]
    fn strip_post_prefix() {
        assert_eq!(strip_hook_prefix("postbuild"), "build");
        assert_eq!(strip_hook_prefix("posttest"), "test");
    }

    #[test]
    fn strip_no_prefix() {
        assert_eq!(strip_hook_prefix("build"), "build");
    }

    // ── parse_as_build_system ───────────────────────────────────────

    #[test]
    fn parse_known_systems() {
        assert!(parse_as_build_system("cargo").is_ok());
        assert!(parse_as_build_system("npm").is_ok());
        assert!(parse_as_build_system("go").is_ok());
        assert!(parse_as_build_system("make").is_ok());
        assert!(parse_as_build_system("just").is_ok());
    }

    #[test]
    fn parse_unknown_system() {
        assert!(parse_as_build_system("docker compose up").is_err());
        assert!(parse_as_build_system("./scripts/gen.sh").is_err());
        assert!(parse_as_build_system("foobar").is_err());
    }

    // ── resolve_hook_value ──────────────────────────────────────────

    #[test]
    fn hook_value_shell_command() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "prebuild",
            HookPhase::Pre,
            &HookValue::Script("./scripts/gen.sh".to_string()),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        assert_eq!(hook.program, "sh");
        assert_eq!(hook.args, vec!["-c", "./scripts/gen.sh"]);
        assert_eq!(hook.display, "./scripts/gen.sh");
        assert_eq!(hook.source, HookSource::Explicit);
    }

    #[test]
    fn hook_value_compound_shell_command() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "pretest",
            HookPhase::Pre,
            &HookValue::Script("docker compose up -d postgres".to_string()),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        assert_eq!(hook.program, "sh");
        assert_eq!(hook.args, vec!["-c", "docker compose up -d postgres"]);
    }

    #[test]
    fn hook_value_cargo_system_for_lint() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "prelint",
            HookPhase::Pre,
            &HookValue::Script("cargo".to_string()),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        // cargo's lint mapping is "cargo clippy"
        assert_eq!(hook.program, "cargo");
        assert_eq!(hook.args, vec!["clippy"]);
    }

    #[test]
    fn hook_value_npm_system_for_build() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "prebuild",
            HookPhase::Pre,
            &HookValue::Script("npm".to_string()),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        // npm's build mapping is "npm run build"
        assert_eq!(hook.program, "npm");
        assert_eq!(hook.args, vec!["run", "build"]);
    }

    #[test]
    fn hook_value_make_system_for_test() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "pretest",
            HookPhase::Pre,
            &HookValue::Script("make".to_string()),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        assert_eq!(hook.program, "make");
        assert_eq!(hook.args, vec!["test"]);
    }

    #[test]
    fn hook_value_argv_executes_directly() {
        let dir = TempDir::new().unwrap();
        let hook = resolve_hook_value(
            "prebuild",
            HookPhase::Pre,
            &HookValue::Argv(vec![
                "pnpm".to_string(),
                "run".to_string(),
                "build".to_string(),
            ]),
            dir.path(),
            HookSource::Explicit,
        )
        .unwrap();
        assert_eq!(hook.program, "pnpm");
        assert_eq!(hook.args, vec!["run", "build"]);
    }

    // ── resolve_hooks (pure resolution) ─────────────────────────────

    #[test]
    fn no_hooks_returns_none() {
        let dir = TempDir::new().unwrap();
        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: HashMap::new(),
            convention_hooks: vec![],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_none());
        assert!(res.post_hook.is_none());
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn explicit_pre_hook_resolved() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "prebuild".to_string(),
            HookValue::Script("./gen.sh".to_string()),
        );

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_some());
        assert!(res.post_hook.is_none());

        let hook = res.pre_hook.unwrap();
        assert_eq!(hook.phase, HookPhase::Pre);
        assert_eq!(hook.name, "prebuild");
        assert_eq!(hook.source, HookSource::Explicit);
        assert_eq!(hook.program, "sh");
    }

    #[test]
    fn explicit_post_hook_resolved() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "posttest".to_string(),
            HookValue::Script("docker compose down".to_string()),
        );

        let input = HookResolveInput {
            command: "test".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_none());
        assert!(res.post_hook.is_some());

        let hook = res.post_hook.unwrap();
        assert_eq!(hook.phase, HookPhase::Post);
        assert_eq!(hook.name, "posttest");
    }

    #[test]
    fn both_pre_and_post_hooks_resolved() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "prebuild".to_string(),
            HookValue::Script("./gen.sh".to_string()),
        );
        explicit.insert(
            "postbuild".to_string(),
            HookValue::Script("./copy-assets.sh".to_string()),
        );

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_some());
        assert!(res.post_hook.is_some());
    }

    #[test]
    fn convention_hook_resolved() {
        let dir = TempDir::new().unwrap();
        let hook_file = HookFile {
            name: "prebuild".to_string(),
            path: dir.path().join("prebuild.sh"),
            executable: true,
        };

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: HashMap::new(),
            convention_hooks: vec![hook_file],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_some());

        let hook = res.pre_hook.unwrap();
        assert_eq!(hook.source, HookSource::Convention);
        assert!(hook.program.contains("prebuild.sh"));
    }

    #[test]
    fn explicit_takes_precedence_over_convention() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "prebuild".to_string(),
            HookValue::Script("echo explicit".to_string()),
        );

        let convention_file = HookFile {
            name: "prebuild".to_string(),
            path: dir.path().join("prebuild.sh"),
            executable: true,
        };

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![convention_file],
        };
        let res = resolve_hooks(&input).unwrap();
        let hook = res.pre_hook.unwrap();
        assert_eq!(hook.source, HookSource::Explicit);
        assert_eq!(hook.display, "echo explicit");
    }

    #[test]
    fn non_executable_convention_hook_skipped_with_warning() {
        let dir = TempDir::new().unwrap();
        let hook_file = HookFile {
            name: "prebuild".to_string(),
            path: dir.path().join("prebuild.sh"),
            executable: false,
        };

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: HashMap::new(),
            convention_hooks: vec![hook_file],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_none());
        assert_eq!(res.warnings.len(), 1);
        assert!(res.warnings[0].message.contains("not executable"));
    }

    #[test]
    fn ambiguous_convention_hooks_return_error() {
        let dir = TempDir::new().unwrap();
        let file_a = HookFile {
            name: "prebuild".to_string(),
            path: dir.path().join("prebuild.sh"),
            executable: true,
        };
        let file_b = HookFile {
            name: "prebuild".to_string(),
            path: dir.path().join("prebuild.py"),
            executable: true,
        };

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: HashMap::new(),
            convention_hooks: vec![file_a, file_b],
        };
        let result = resolve_hooks(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            RunError::AmbiguousHook {
                hook_name,
                candidates,
            } => {
                assert_eq!(hook_name, "prebuild");
                assert_eq!(candidates.len(), 2);
            }
            other => panic!("expected AmbiguousHook, got: {other}"),
        }
    }

    #[test]
    fn hooks_for_unrelated_command_not_resolved() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "prebuild".to_string(),
            HookValue::Script("echo build hook".to_string()),
        );

        let input = HookResolveInput {
            command: "test".to_string(), // asking for test, but only build hook exists
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_none());
        assert!(res.post_hook.is_none());
    }

    #[test]
    fn mixed_explicit_and_convention_hooks() {
        let dir = TempDir::new().unwrap();
        let mut explicit = HashMap::new();
        explicit.insert(
            "prebuild".to_string(),
            HookValue::Script("echo pre".to_string()),
        );

        let convention_file = HookFile {
            name: "postbuild".to_string(),
            path: dir.path().join("postbuild.sh"),
            executable: true,
        };

        let input = HookResolveInput {
            command: "build".to_string(),
            package_dir: dir.path().to_path_buf(),
            explicit_hooks: explicit,
            convention_hooks: vec![convention_file],
        };
        let res = resolve_hooks(&input).unwrap();
        assert!(res.pre_hook.is_some());
        assert!(res.post_hook.is_some());
        assert_eq!(res.pre_hook.unwrap().source, HookSource::Explicit);
        assert_eq!(res.post_hook.unwrap().source, HookSource::Convention);
    }

    // ── scan_hooks_dir (filesystem) ─────────────────────────────────

    #[test]
    fn scan_nonexistent_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".buona/hooks");
        assert!(scan_hooks_dir(&hooks_dir).is_empty());
    }

    #[test]
    fn scan_empty_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        assert!(scan_hooks_dir(&hooks_dir).is_empty());
    }

    #[test]
    fn scan_finds_hook_files_by_stem() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("prebuild.sh"), "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks_dir.join("prebuild.sh"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let results = scan_hooks_dir(&hooks_dir);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "prebuild");
        assert!(results[0].executable);
    }

    #[test]
    fn scan_detects_non_executable_files() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("prebuild.sh"), "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks_dir.join("prebuild.sh"),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }

        let results = scan_hooks_dir(&hooks_dir);
        assert_eq!(results.len(), 1);
        #[cfg(unix)]
        assert!(!results[0].executable);
    }

    #[test]
    fn scan_ignores_directories() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(hooks_dir.join("prebuild")).unwrap();

        let results = scan_hooks_dir(&hooks_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn scan_finds_multiple_files_for_same_stem() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("prebuild.sh"), "#!/bin/sh\n").unwrap();
        std::fs::write(hooks_dir.join("prebuild.py"), "#!/usr/bin/env python\n").unwrap();

        let results = scan_hooks_dir(&hooks_dir);
        let prebuild_count = results.iter().filter(|f| f.name == "prebuild").count();
        assert_eq!(prebuild_count, 2);
    }

    #[test]
    fn scan_finds_extensionless_files() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("prebuild"), "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks_dir.join("prebuild"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let results = scan_hooks_dir(&hooks_dir);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "prebuild");
    }
}
