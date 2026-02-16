//! Build system auto-detection by scanning for marker files.

use std::fs;
use std::path::Path;

use super::systems::{marker_files, refine_python_system};
use super::types::BuildSystem;

/// A detected build system with the marker file that triggered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Detection {
    pub(crate) system: BuildSystem,
    pub(crate) marker: String,
}

/// Auto-detect the build system by scanning for marker files in the given directory.
///
/// Returns `None` if no known marker file is found. Marker files are checked in
/// priority order (see [`marker_files()`]).
pub(super) fn detect_build_system(dir: &Path) -> Option<BuildSystem> {
    for &(marker, system) in marker_files() {
        if dir.join(marker).exists() {
            // pyproject.toml could be uv or poetry — refine by inspecting content
            if marker == "pyproject.toml"
                && let Ok(content) = fs::read_to_string(dir.join(marker))
            {
                return Some(refine_python_system(&content));
            }
            return Some(system);
        }
    }
    None
}

/// Scan for all matching marker files, returning every detection in priority order.
///
/// The first entry in the returned vec is the winner. Subsequent entries are
/// lower-priority matches. Returns an empty vec if nothing is found.
pub(crate) fn detect_all_systems(dir: &Path) -> Vec<Detection> {
    let mut results = Vec::new();
    for &(marker, system) in marker_files() {
        if dir.join(marker).exists() {
            let resolved = if marker == "pyproject.toml" {
                if let Ok(content) = fs::read_to_string(dir.join(marker)) {
                    refine_python_system(&content)
                } else {
                    system
                }
            } else {
                system
            };
            results.push(Detection {
                system: resolved,
                marker: marker.to_string(),
            });
        }
    }
    results
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
    fn detects_just_from_justfile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("justfile"), "default:\n\techo hi").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Just));
    }

    #[test]
    fn detects_just_from_dot_justfile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".justfile"), "default:\n\techo hi").unwrap();

        assert_eq!(detect_build_system(dir.path()), Some(BuildSystem::Just));
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

    // ── detect_all_systems tests ───────────────────────────────

    #[test]
    fn detect_all_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert!(detect_all_systems(dir.path()).is_empty());
    }

    #[test]
    fn detect_all_single_system() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let results = detect_all_systems(dir.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].system, BuildSystem::Cargo);
        assert_eq!(results[0].marker, "Cargo.toml");
    }

    #[test]
    fn detect_all_cargo_and_makefile() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();

        let results = detect_all_systems(dir.path());
        assert_eq!(results.len(), 2);
        // Cargo has higher priority
        assert_eq!(results[0].system, BuildSystem::Cargo);
        assert_eq!(results[1].system, BuildSystem::Make);
    }

    #[test]
    fn detect_all_pnpm_also_finds_package_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let results = detect_all_systems(dir.path());
        // Should find pnpm-lock.yaml (Pnpm) and package.json (Npm)
        assert!(results.len() >= 2);
        assert_eq!(results[0].system, BuildSystem::Pnpm);
        // package.json also appears as a lower-priority Npm detection
        assert!(results.iter().any(|d| d.system == BuildSystem::Npm));
    }

    #[test]
    fn detect_all_preserves_priority_order() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example").unwrap();
        fs::write(dir.path().join("Makefile"), "all:").unwrap();

        let results = detect_all_systems(dir.path());
        assert_eq!(results.len(), 2);
        // go.mod has higher priority than Makefile
        assert_eq!(results[0].system, BuildSystem::Go);
        assert_eq!(results[1].system, BuildSystem::Make);
    }

    #[test]
    fn detect_all_refines_pyproject_to_poetry() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.poetry]\nname = \"my-project\"\n",
        )
        .unwrap();

        let results = detect_all_systems(dir.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].system, BuildSystem::Poetry);
        assert_eq!(results[0].marker, "pyproject.toml");
    }
}
