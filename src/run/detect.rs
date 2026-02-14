//! Build system auto-detection by scanning for marker files.

use std::fs;
use std::path::Path;

use super::systems::{marker_files, refine_python_system};
use super::types::BuildSystem;

/// Auto-detect the build system by scanning for marker files in the given directory.
///
/// Returns `None` if no known marker file is found. Marker files are checked in
/// priority order (see [`marker_files()`]).
pub(super) fn detect_build_system(dir: &Path) -> Option<BuildSystem> {
    for &(marker, system) in marker_files() {
        if dir.join(marker).exists() {
            // pyproject.toml could be uv or poetry — refine by inspecting content
            if marker == "pyproject.toml" {
                if let Ok(content) = fs::read_to_string(dir.join(marker)) {
                    return Some(refine_python_system(&content));
                }
            }
            return Some(system);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detects_cargo() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Cargo));
    }

    #[test]
    fn detects_go() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Go));
    }

    #[test]
    fn detects_npm_from_package_lock() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Npm));
    }

    #[test]
    fn detects_pnpm_over_npm() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        // pnpm-lock.yaml has higher priority than package.json
        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Pnpm));
    }

    #[test]
    fn detects_bun_from_lockfile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("bun.lock"), "").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Bun));
    }

    #[test]
    fn detects_yarn_from_lockfile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Yarn));
    }

    #[test]
    fn detects_npm_from_package_json_alone() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Npm));
    }

    #[test]
    fn detects_uv_from_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"my-project\"\n",
        )
        .unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Uv));
    }

    #[test]
    fn detects_poetry_from_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.poetry]\nname = \"my-project\"\n",
        )
        .unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Poetry));
    }

    #[test]
    fn detects_make() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Makefile"), "all:\n\techo hi").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Make));
    }

    #[test]
    fn detects_gradle_from_build_gradle() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.gradle"), "plugins {}").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Gradle));
    }

    #[test]
    fn detects_gradle_from_build_gradle_kts() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.gradle.kts"), "plugins {}").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Gradle));
    }

    #[test]
    fn detects_maven_from_pom_xml() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pom.xml"), "<project></project>").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Maven));
    }

    #[test]
    fn returns_none_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_build_system(dir.path()), None);
    }

    #[test]
    fn cargo_takes_priority_over_makefile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Cargo));
    }
}
