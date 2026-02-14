//! Build system definitions: marker files, standard command mappings, and proxy behavior.

use std::path::Path;

use super::types::{BuildSystem, StandardCommand};

/// Marker files used to auto-detect a build system.
///
/// Returns `(marker_filename, build_system)` pairs ordered by priority.
/// Earlier entries take precedence when multiple markers exist.
pub(super) fn marker_files() -> &'static [(&'static str, BuildSystem)] {
    &[
        ("Cargo.toml", BuildSystem::Cargo),
        ("go.mod", BuildSystem::Go),
        ("bun.lock", BuildSystem::Bun),
        ("bun.lockb", BuildSystem::Bun),
        ("pnpm-lock.yaml", BuildSystem::Pnpm),
        ("yarn.lock", BuildSystem::Yarn),
        ("package-lock.json", BuildSystem::Npm),
        ("package.json", BuildSystem::Npm),
        ("build.gradle.kts", BuildSystem::Gradle),
        ("build.gradle", BuildSystem::Gradle),
        ("pom.xml", BuildSystem::Maven),
        ("pyproject.toml", BuildSystem::Uv),
        ("Makefile", BuildSystem::Make),
    ]
}

/// For `pyproject.toml`, distinguish between uv and poetry by checking content.
pub(super) fn refine_python_system(pyproject_content: &str) -> BuildSystem {
    if pyproject_content.contains("[tool.poetry]") {
        BuildSystem::Poetry
    } else {
        BuildSystem::Uv
    }
}

/// Detect the gradle program to use, preferring `./gradlew` if present.
fn detect_gradle_program(dir: Option<&Path>) -> String {
    if let Some(d) = dir {
        if d.join("gradlew").exists() {
            return "./gradlew".to_string();
        }
    }
    "gradle".to_string()
}

/// Detect the maven program to use, preferring `./mvnw` if present.
fn detect_maven_program(dir: Option<&Path>) -> String {
    if let Some(d) = dir {
        if d.join("mvnw").exists() {
            return "./mvnw".to_string();
        }
    }
    "mvn".to_string()
}

/// Map a standard command to `(program, args)` for a given build system.
///
/// Returns `None` if the standard command has no mapping for this system.
/// The `package_dir` is needed for Gradle/Maven to detect wrapper scripts.
pub(super) fn standard_mapping(
    system: BuildSystem,
    cmd: StandardCommand,
    extra_args: &[String],
    package_dir: Option<&Path>,
) -> Option<(String, Vec<String>)> {
    let (program, base_args): (&str, Vec<&str>) = match (system, cmd) {
        // ── Cargo ────────────────────────────────────
        (BuildSystem::Cargo, StandardCommand::Install) => ("cargo", vec!["build"]),
        (BuildSystem::Cargo, StandardCommand::Build) => ("cargo", vec!["build"]),
        (BuildSystem::Cargo, StandardCommand::Run) => ("cargo", vec!["run"]),
        (BuildSystem::Cargo, StandardCommand::Test) => ("cargo", vec!["test"]),
        (BuildSystem::Cargo, StandardCommand::Lint) => ("cargo", vec!["clippy"]),
        (BuildSystem::Cargo, StandardCommand::Fmt) => ("cargo", vec!["fmt"]),
        (BuildSystem::Cargo, StandardCommand::Clean) => ("cargo", vec!["clean"]),
        (BuildSystem::Cargo, StandardCommand::Publish) => ("cargo", vec!["publish"]),
        (BuildSystem::Cargo, StandardCommand::Bench) => ("cargo", vec!["bench"]),
        (BuildSystem::Cargo, StandardCommand::Doc) => ("cargo", vec!["doc"]),
        (BuildSystem::Cargo, StandardCommand::Dev) => return None,

        // ── Go ───────────────────────────────────────
        (BuildSystem::Go, StandardCommand::Install) => ("go", vec!["mod", "download"]),
        (BuildSystem::Go, StandardCommand::Build) => ("go", vec!["build", "./..."]),
        (BuildSystem::Go, StandardCommand::Run) => ("go", vec!["run", "."]),
        (BuildSystem::Go, StandardCommand::Test) => ("go", vec!["test", "./..."]),
        (BuildSystem::Go, StandardCommand::Lint) => ("golangci-lint", vec!["run"]),
        (BuildSystem::Go, StandardCommand::Fmt) => ("gofmt", vec!["-w", "."]),
        (BuildSystem::Go, StandardCommand::Clean) => ("go", vec!["clean"]),
        (BuildSystem::Go, StandardCommand::Doc) => ("go", vec!["doc", "./..."]),
        (BuildSystem::Go, _) => return None,

        // ── npm ──────────────────────────────────────
        (BuildSystem::Npm, StandardCommand::Install) => ("npm", vec!["install"]),
        (BuildSystem::Npm, StandardCommand::Build) => ("npm", vec!["run", "build"]),
        (BuildSystem::Npm, StandardCommand::Run) => ("npm", vec!["start"]),
        (BuildSystem::Npm, StandardCommand::Test) => ("npm", vec!["test"]),
        (BuildSystem::Npm, StandardCommand::Lint) => ("npm", vec!["run", "lint"]),
        (BuildSystem::Npm, StandardCommand::Fmt) => ("npm", vec!["run", "fmt"]),
        (BuildSystem::Npm, StandardCommand::Clean) => ("npm", vec!["run", "clean"]),
        (BuildSystem::Npm, StandardCommand::Publish) => ("npm", vec!["publish"]),
        (BuildSystem::Npm, StandardCommand::Dev) => ("npm", vec!["run", "dev"]),
        (BuildSystem::Npm, _) => return None,

        // ── pnpm ─────────────────────────────────────
        (BuildSystem::Pnpm, StandardCommand::Install) => ("pnpm", vec!["install"]),
        (BuildSystem::Pnpm, StandardCommand::Build) => ("pnpm", vec!["build"]),
        (BuildSystem::Pnpm, StandardCommand::Run) => ("pnpm", vec!["start"]),
        (BuildSystem::Pnpm, StandardCommand::Test) => ("pnpm", vec!["test"]),
        (BuildSystem::Pnpm, StandardCommand::Lint) => ("pnpm", vec!["lint"]),
        (BuildSystem::Pnpm, StandardCommand::Fmt) => ("pnpm", vec!["fmt"]),
        (BuildSystem::Pnpm, StandardCommand::Clean) => ("pnpm", vec!["clean"]),
        (BuildSystem::Pnpm, StandardCommand::Publish) => ("pnpm", vec!["publish"]),
        (BuildSystem::Pnpm, StandardCommand::Dev) => ("pnpm", vec!["dev"]),
        (BuildSystem::Pnpm, _) => return None,

        // ── yarn ─────────────────────────────────────
        (BuildSystem::Yarn, StandardCommand::Install) => ("yarn", vec!["install"]),
        (BuildSystem::Yarn, StandardCommand::Build) => ("yarn", vec!["build"]),
        (BuildSystem::Yarn, StandardCommand::Run) => ("yarn", vec!["start"]),
        (BuildSystem::Yarn, StandardCommand::Test) => ("yarn", vec!["test"]),
        (BuildSystem::Yarn, StandardCommand::Lint) => ("yarn", vec!["lint"]),
        (BuildSystem::Yarn, StandardCommand::Fmt) => ("yarn", vec!["fmt"]),
        (BuildSystem::Yarn, StandardCommand::Clean) => ("yarn", vec!["clean"]),
        (BuildSystem::Yarn, StandardCommand::Publish) => ("yarn", vec!["publish"]),
        (BuildSystem::Yarn, StandardCommand::Dev) => ("yarn", vec!["dev"]),
        (BuildSystem::Yarn, _) => return None,

        // ── bun ──────────────────────────────────────
        (BuildSystem::Bun, StandardCommand::Install) => ("bun", vec!["install"]),
        (BuildSystem::Bun, StandardCommand::Build) => ("bun", vec!["run", "build"]),
        (BuildSystem::Bun, StandardCommand::Run) => ("bun", vec!["start"]),
        (BuildSystem::Bun, StandardCommand::Test) => ("bun", vec!["test"]),
        (BuildSystem::Bun, StandardCommand::Lint) => ("bun", vec!["run", "lint"]),
        (BuildSystem::Bun, StandardCommand::Fmt) => ("bun", vec!["run", "fmt"]),
        (BuildSystem::Bun, StandardCommand::Clean) => ("bun", vec!["run", "clean"]),
        (BuildSystem::Bun, StandardCommand::Publish) => ("bun", vec!["publish"]),
        (BuildSystem::Bun, StandardCommand::Dev) => ("bun", vec!["run", "dev"]),
        (BuildSystem::Bun, _) => return None,

        // ── uv ───────────────────────────────────────
        (BuildSystem::Uv, StandardCommand::Install) => ("uv", vec!["sync"]),
        (BuildSystem::Uv, StandardCommand::Build) => ("uv", vec!["build"]),
        (BuildSystem::Uv, StandardCommand::Run) => ("uv", vec!["run", "python", "-m"]),
        (BuildSystem::Uv, StandardCommand::Test) => ("uv", vec!["run", "pytest"]),
        (BuildSystem::Uv, StandardCommand::Lint) => ("uv", vec!["run", "ruff", "check", "."]),
        (BuildSystem::Uv, StandardCommand::Fmt) => ("uv", vec!["run", "ruff", "format", "."]),
        (BuildSystem::Uv, StandardCommand::Publish) => ("uv", vec!["publish"]),
        (BuildSystem::Uv, _) => return None,

        // ── poetry ───────────────────────────────────
        (BuildSystem::Poetry, StandardCommand::Install) => ("poetry", vec!["install"]),
        (BuildSystem::Poetry, StandardCommand::Build) => ("poetry", vec!["build"]),
        (BuildSystem::Poetry, StandardCommand::Run) => ("poetry", vec!["run", "python", "-m"]),
        (BuildSystem::Poetry, StandardCommand::Test) => ("poetry", vec!["run", "pytest"]),
        (BuildSystem::Poetry, StandardCommand::Lint) => {
            ("poetry", vec!["run", "ruff", "check", "."])
        }
        (BuildSystem::Poetry, StandardCommand::Fmt) => {
            ("poetry", vec!["run", "ruff", "format", "."])
        }
        (BuildSystem::Poetry, StandardCommand::Publish) => ("poetry", vec!["publish"]),
        (BuildSystem::Poetry, _) => return None,

        // ── make ─────────────────────────────────────
        (BuildSystem::Make, StandardCommand::Build) => ("make", vec![]),
        (BuildSystem::Make, StandardCommand::Test) => ("make", vec!["test"]),
        (BuildSystem::Make, StandardCommand::Clean) => ("make", vec!["clean"]),
        (BuildSystem::Make, StandardCommand::Install) => ("make", vec!["install"]),
        (BuildSystem::Make, _) => return None,

        // ── gradle ───────────────────────────────────
        (BuildSystem::Gradle, StandardCommand::Build) => return Some(gradle_mapping("build", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Test) => return Some(gradle_mapping("test", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Clean) => return Some(gradle_mapping("clean", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Install) => return Some(gradle_mapping("assemble", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Lint) => return Some(gradle_mapping("check", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Publish) => return Some(gradle_mapping("publish", extra_args, package_dir)),
        (BuildSystem::Gradle, StandardCommand::Doc) => return Some(gradle_mapping("javadoc", extra_args, package_dir)),
        (BuildSystem::Gradle, _) => return None,

        // ── maven ────────────────────────────────────
        (BuildSystem::Maven, StandardCommand::Build) => return Some(maven_mapping("compile", extra_args, package_dir)),
        (BuildSystem::Maven, StandardCommand::Test) => return Some(maven_mapping("test", extra_args, package_dir)),
        (BuildSystem::Maven, StandardCommand::Clean) => return Some(maven_mapping("clean", extra_args, package_dir)),
        (BuildSystem::Maven, StandardCommand::Install) => return Some(maven_mapping("install", extra_args, package_dir)),
        (BuildSystem::Maven, StandardCommand::Publish) => return Some(maven_mapping("deploy", extra_args, package_dir)),
        (BuildSystem::Maven, StandardCommand::Doc) => return Some(maven_mapping("javadoc:javadoc", extra_args, package_dir)),
        (BuildSystem::Maven, _) => return None,
    };

    let mut args: Vec<String> = base_args.into_iter().map(String::from).collect();
    if !extra_args.is_empty() {
        // For cargo/go, insert -- before extra args for standard commands
        if matches!(system, BuildSystem::Cargo | BuildSystem::Go) {
            args.push("--".to_string());
        }
        args.extend(extra_args.iter().cloned());
    }
    Some((program.to_string(), args))
}

/// Build a gradle standard command mapping with wrapper detection.
fn gradle_mapping(
    task: &str,
    extra_args: &[String],
    package_dir: Option<&Path>,
) -> (String, Vec<String>) {
    let program = detect_gradle_program(package_dir);
    let mut args = vec![task.to_string()];
    args.extend(extra_args.iter().cloned());
    (program, args)
}

/// Build a maven standard command mapping with wrapper detection.
fn maven_mapping(
    goal: &str,
    extra_args: &[String],
    package_dir: Option<&Path>,
) -> (String, Vec<String>) {
    let program = detect_maven_program(package_dir);
    let mut args = vec![goal.to_string()];
    args.extend(extra_args.iter().cloned());
    (program, args)
}

/// Build a proxy command for an unrecognized command name.
///
/// Returns `(program, args)`. The `package_dir` is needed for Gradle/Maven wrapper detection.
pub(super) fn proxy_command(
    system: BuildSystem,
    command: &str,
    extra_args: &[String],
    package_dir: Option<&Path>,
) -> (String, Vec<String>) {
    let (program, mut args) = match system {
        // npm: builtins pass through directly, else "npm run <cmd>"
        BuildSystem::Npm => {
            if is_npm_builtin(command) {
                ("npm".to_string(), vec![command.to_string()])
            } else {
                (
                    "npm".to_string(),
                    vec!["run".to_string(), command.to_string()],
                )
            }
        }
        // bun: builtins pass through, else "bun run <cmd>"
        BuildSystem::Bun => {
            if is_bun_builtin(command) {
                ("bun".to_string(), vec![command.to_string()])
            } else {
                (
                    "bun".to_string(),
                    vec!["run".to_string(), command.to_string()],
                )
            }
        }
        // pnpm/yarn: always "<tool> <cmd>"
        BuildSystem::Pnpm => ("pnpm".to_string(), vec![command.to_string()]),
        BuildSystem::Yarn => ("yarn".to_string(), vec![command.to_string()]),
        // Non-JS systems: "<tool> <cmd>"
        BuildSystem::Cargo => ("cargo".to_string(), vec![command.to_string()]),
        BuildSystem::Go => ("go".to_string(), vec![command.to_string()]),
        BuildSystem::Uv => (
            "uv".to_string(),
            vec!["run".to_string(), command.to_string()],
        ),
        BuildSystem::Poetry => (
            "poetry".to_string(),
            vec!["run".to_string(), command.to_string()],
        ),
        BuildSystem::Make => ("make".to_string(), vec![command.to_string()]),
        BuildSystem::Gradle => {
            let program = detect_gradle_program(package_dir);
            (program, vec![command.to_string()])
        }
        BuildSystem::Maven => {
            let program = detect_maven_program(package_dir);
            (program, vec![command.to_string()])
        }
    };
    args.extend(extra_args.iter().cloned());
    (program, args)
}

fn is_npm_builtin(cmd: &str) -> bool {
    matches!(
        cmd,
        "install"
            | "ci"
            | "test"
            | "start"
            | "stop"
            | "restart"
            | "publish"
            | "pack"
            | "init"
            | "version"
            | "uninstall"
            | "update"
            | "outdated"
            | "ls"
            | "link"
            | "audit"
    )
}

fn is_bun_builtin(cmd: &str) -> bool {
    matches!(
        cmd,
        "install"
            | "add"
            | "remove"
            | "update"
            | "link"
            | "test"
            | "init"
            | "create"
            | "upgrade"
            | "pm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── standard_mapping tests ───────────────────────────────────

    #[test]
    fn cargo_test_mapping() {
        let (prog, args) = standard_mapping(
            BuildSystem::Cargo,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "cargo");
        assert_eq!(args, vec!["test"]);
    }

    #[test]
    fn cargo_test_with_extra_args() {
        let extra = vec!["--nocapture".to_string()];
        let (prog, args) = standard_mapping(
            BuildSystem::Cargo,
            StandardCommand::Test,
            &extra,
            None,
        )
        .unwrap();
        assert_eq!(prog, "cargo");
        assert_eq!(args, vec!["test", "--", "--nocapture"]);
    }

    #[test]
    fn cargo_lint_maps_to_clippy() {
        let (prog, args) = standard_mapping(
            BuildSystem::Cargo,
            StandardCommand::Lint,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "cargo");
        assert_eq!(args, vec!["clippy"]);
    }

    #[test]
    fn cargo_dev_returns_none() {
        let result = standard_mapping(
            BuildSystem::Cargo,
            StandardCommand::Dev,
            &[],
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn npm_build_uses_run() {
        let (prog, args) = standard_mapping(
            BuildSystem::Npm,
            StandardCommand::Build,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "npm");
        assert_eq!(args, vec!["run", "build"]);
    }

    #[test]
    fn npm_install_is_direct() {
        let (prog, args) = standard_mapping(
            BuildSystem::Npm,
            StandardCommand::Install,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "npm");
        assert_eq!(args, vec!["install"]);
    }

    #[test]
    fn pnpm_test_is_direct() {
        let (prog, args) = standard_mapping(
            BuildSystem::Pnpm,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "pnpm");
        assert_eq!(args, vec!["test"]);
    }

    #[test]
    fn go_test_mapping() {
        let (prog, args) = standard_mapping(
            BuildSystem::Go,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "go");
        assert_eq!(args, vec!["test", "./..."]);
    }

    #[test]
    fn go_lint_uses_golangci() {
        let (prog, args) = standard_mapping(
            BuildSystem::Go,
            StandardCommand::Lint,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "golangci-lint");
        assert_eq!(args, vec!["run"]);
    }

    #[test]
    fn uv_test_mapping() {
        let (prog, args) = standard_mapping(
            BuildSystem::Uv,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "uv");
        assert_eq!(args, vec!["run", "pytest"]);
    }

    #[test]
    fn make_build_has_no_target() {
        let (prog, args) = standard_mapping(
            BuildSystem::Make,
            StandardCommand::Build,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "make");
        assert!(args.is_empty());
    }

    #[test]
    fn make_bench_returns_none() {
        let result = standard_mapping(
            BuildSystem::Make,
            StandardCommand::Bench,
            &[],
            None,
        );
        assert!(result.is_none());
    }

    // ── gradle tests ─────────────────────────────────────────────

    #[test]
    fn gradle_build_without_wrapper() {
        let (prog, args) = standard_mapping(
            BuildSystem::Gradle,
            StandardCommand::Build,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "gradle");
        assert_eq!(args, vec!["build"]);
    }

    #[test]
    fn gradle_build_with_wrapper() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("gradlew"), "#!/bin/sh").unwrap();

        let (prog, args) = standard_mapping(
            BuildSystem::Gradle,
            StandardCommand::Build,
            &[],
            Some(dir.path()),
        )
        .unwrap();
        assert_eq!(prog, "./gradlew");
        assert_eq!(args, vec!["build"]);
    }

    #[test]
    fn gradle_test_mapping() {
        let (prog, args) = standard_mapping(
            BuildSystem::Gradle,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "gradle");
        assert_eq!(args, vec!["test"]);
    }

    #[test]
    fn gradle_install_maps_to_assemble() {
        let (prog, args) = standard_mapping(
            BuildSystem::Gradle,
            StandardCommand::Install,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "gradle");
        assert_eq!(args, vec!["assemble"]);
    }

    #[test]
    fn gradle_lint_maps_to_check() {
        let (prog, args) = standard_mapping(
            BuildSystem::Gradle,
            StandardCommand::Lint,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "gradle");
        assert_eq!(args, vec!["check"]);
    }

    // ── maven tests ──────────────────────────────────────────────

    #[test]
    fn maven_build_without_wrapper() {
        let (prog, args) = standard_mapping(
            BuildSystem::Maven,
            StandardCommand::Build,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "mvn");
        assert_eq!(args, vec!["compile"]);
    }

    #[test]
    fn maven_build_with_wrapper() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("mvnw"), "#!/bin/sh").unwrap();

        let (prog, args) = standard_mapping(
            BuildSystem::Maven,
            StandardCommand::Build,
            &[],
            Some(dir.path()),
        )
        .unwrap();
        assert_eq!(prog, "./mvnw");
        assert_eq!(args, vec!["compile"]);
    }

    #[test]
    fn maven_test_mapping() {
        let (prog, args) = standard_mapping(
            BuildSystem::Maven,
            StandardCommand::Test,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "mvn");
        assert_eq!(args, vec!["test"]);
    }

    #[test]
    fn maven_publish_maps_to_deploy() {
        let (prog, args) = standard_mapping(
            BuildSystem::Maven,
            StandardCommand::Publish,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "mvn");
        assert_eq!(args, vec!["deploy"]);
    }

    #[test]
    fn maven_doc_maps_to_javadoc() {
        let (prog, args) = standard_mapping(
            BuildSystem::Maven,
            StandardCommand::Doc,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(prog, "mvn");
        assert_eq!(args, vec!["javadoc:javadoc"]);
    }

    // ── proxy_command tests ──────────────────────────────────────

    #[test]
    fn npm_proxy_builtin_passes_through() {
        let (prog, args) = proxy_command(BuildSystem::Npm, "audit", &[], None);
        assert_eq!(prog, "npm");
        assert_eq!(args, vec!["audit"]);
    }

    #[test]
    fn npm_proxy_non_builtin_uses_run() {
        let (prog, args) = proxy_command(BuildSystem::Npm, "my-script", &[], None);
        assert_eq!(prog, "npm");
        assert_eq!(args, vec!["run", "my-script"]);
    }

    #[test]
    fn bun_proxy_builtin_passes_through() {
        let (prog, args) = proxy_command(BuildSystem::Bun, "add", &[], None);
        assert_eq!(prog, "bun");
        assert_eq!(args, vec!["add"]);
    }

    #[test]
    fn bun_proxy_non_builtin_uses_run() {
        let (prog, args) = proxy_command(BuildSystem::Bun, "my-script", &[], None);
        assert_eq!(prog, "bun");
        assert_eq!(args, vec!["run", "my-script"]);
    }

    #[test]
    fn pnpm_proxy_passes_through() {
        let (prog, args) = proxy_command(BuildSystem::Pnpm, "custom", &[], None);
        assert_eq!(prog, "pnpm");
        assert_eq!(args, vec!["custom"]);
    }

    #[test]
    fn cargo_proxy_passes_through() {
        let (prog, args) = proxy_command(BuildSystem::Cargo, "asm", &[], None);
        assert_eq!(prog, "cargo");
        assert_eq!(args, vec!["asm"]);
    }

    #[test]
    fn proxy_appends_extra_args() {
        let extra = vec!["--flag".to_string(), "value".to_string()];
        let (prog, args) = proxy_command(BuildSystem::Npm, "my-script", &extra, None);
        assert_eq!(prog, "npm");
        assert_eq!(args, vec!["run", "my-script", "--flag", "value"]);
    }

    #[test]
    fn gradle_proxy_without_wrapper() {
        let (prog, args) = proxy_command(BuildSystem::Gradle, "spotless", &[], None);
        assert_eq!(prog, "gradle");
        assert_eq!(args, vec!["spotless"]);
    }

    #[test]
    fn gradle_proxy_with_wrapper() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("gradlew"), "#!/bin/sh").unwrap();

        let (prog, args) = proxy_command(BuildSystem::Gradle, "spotless", &[], Some(dir.path()));
        assert_eq!(prog, "./gradlew");
        assert_eq!(args, vec!["spotless"]);
    }

    #[test]
    fn maven_proxy_without_wrapper() {
        let (prog, args) = proxy_command(BuildSystem::Maven, "verify", &[], None);
        assert_eq!(prog, "mvn");
        assert_eq!(args, vec!["verify"]);
    }

    // ── refine_python_system tests ───────────────────────────────

    #[test]
    fn refine_python_with_poetry() {
        let content = r#"
[tool.poetry]
name = "my-project"
"#;
        assert_eq!(refine_python_system(content), BuildSystem::Poetry);
    }

    #[test]
    fn refine_python_without_poetry() {
        let content = r#"
[project]
name = "my-project"
"#;
        assert_eq!(refine_python_system(content), BuildSystem::Uv);
    }

    // ── npm/bun builtin tests ────────────────────────────────────

    #[test]
    fn npm_builtins() {
        assert!(is_npm_builtin("install"));
        assert!(is_npm_builtin("ci"));
        assert!(is_npm_builtin("test"));
        assert!(is_npm_builtin("audit"));
        assert!(!is_npm_builtin("dev"));
        assert!(!is_npm_builtin("build"));
    }

    #[test]
    fn bun_builtins() {
        assert!(is_bun_builtin("install"));
        assert!(is_bun_builtin("add"));
        assert!(is_bun_builtin("test"));
        assert!(!is_bun_builtin("dev"));
        assert!(!is_bun_builtin("build"));
    }
}
