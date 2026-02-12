use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::config::GitProtocol;
use crate::styles::Styles;

const WORKSPACE_FILE: &str = "buona.workspace.json";

/// A tracked package that has been added to a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageEntry {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub name: String,

    /// Packages added to this workspace via `buona ws add`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<PackageEntry>,
}

/// Read workspace metadata from a directory, if a `buona.workspace.json` exists.
pub fn read_meta(dir: &Path) -> Option<WorkspaceMeta> {
    let path = dir.join(WORKSPACE_FILE);
    let contents = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Write workspace metadata to the given directory.
fn write_meta(dir: &Path, meta: &WorkspaceMeta) -> Result<()> {
    let meta_path = dir.join(WORKSPACE_FILE);
    let json = serde_json::to_string_pretty(meta)?;
    fs::write(&meta_path, json + "\n")
        .with_context(|| format!("could not write {WORKSPACE_FILE}"))?;
    Ok(())
}

/// Find a workspace by name or directory name. Returns the resolved path.
fn find_workspace(query: &str) -> Result<PathBuf> {
    let workspace_dir = config::workspace_dir()?;

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct).is_some() {
        return Ok(direct);
    }

    // Otherwise, search by workspace name in metadata
    let entries = fs::read_dir(&workspace_dir)
        .with_context(|| {
            format!(
                "could not read workspace directory: {}",
                workspace_dir.display()
            )
        })?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path) {
                if meta.name == query {
                    return Ok(path);
                }
            }
        }
    }

    bail!("no workspace found matching \"{query}\"")
}

/// Walk up from the given directory looking for a `buona.workspace.json` file.
/// Returns the directory containing the workspace file.
fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(WORKSPACE_FILE).exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!(
                "not inside a workspace (no {} found in any parent directory)\n  \
                 Either cd into a workspace or use --workspace to specify one.",
                WORKSPACE_FILE
            );
        }
    }
}

/// Find the workspace root from the current working directory.
fn find_workspace_from_cwd() -> Result<PathBuf> {
    let cwd = env::current_dir().context("could not determine current directory")?;
    find_workspace_root(&cwd)
}

/// Resolved package information: the git clone URL and the local directory name.
struct ResolvedPackage {
    url: String,
    name: String,
}

/// Build a git clone URL from components.
fn build_clone_url(host: &str, org: &str, package: &str, protocol: GitProtocol) -> String {
    match protocol {
        GitProtocol::Ssh => format!("git@{host}:{org}/{package}.git"),
        GitProtocol::Https => format!("https://{host}/{org}/{package}.git"),
    }
}

/// Extract the package name from a URL or path.
/// Takes the last path segment and strips any `.git` suffix.
fn extract_package_name(url: &str) -> String {
    // Split by '/' to get the last segment
    let segment = url.rsplit('/').next().unwrap_or(url);
    // For SSH URLs like git@host:repo.git (no slash after colon), also split by ':'
    let segment = if segment.contains(':') {
        segment.rsplit(':').next().unwrap_or(segment)
    } else {
        segment
    };
    segment.strip_suffix(".git").unwrap_or(segment).to_string()
}

/// Parse a package specifier into a resolved git URL and package name.
///
/// Supports three patterns:
/// 1. Fully qualified URL (contains `://` or starts with `git@`)
/// 2. Org/Package (contains exactly one `/`)
/// 3. Direct package name (no `/`)
fn resolve_package_spec(spec: &str, git: &config::GitConfig) -> Result<ResolvedPackage> {
    let url = if spec.contains("://") || spec.starts_with("git@") {
        // Fully qualified URL — use as-is
        spec.to_string()
    } else if spec.contains('/') {
        // Org/Package pattern
        let (org, package) = spec
            .split_once('/')
            .expect("already checked contains '/'");
        build_clone_url(&git.host, org, package, git.protocol)
    } else {
        // Direct package name — requires organization in config
        if git.organization.is_empty() {
            bail!(
                "cannot resolve package \"{spec}\" without a configured git organization.\n  \
                 Run `buona config setup` to set one, or use the org/package format."
            );
        }
        build_clone_url(&git.host, &git.organization, spec, git.protocol)
    };

    let name = extract_package_name(&url);
    Ok(ResolvedPackage { url, name })
}

/// List all workspaces (directories) found in the configured workspace directory.
pub fn list() -> Result<()> {
    let workspace_dir = config::workspace_dir()?;
    let s = Styles::default();

    println!();
    println!("  {}", s.bold.apply_to("Workspaces"));
    println!("  {}", s.dim.apply_to("──────────"));

    if !workspace_dir.exists() {
        bail!(
            "workspace directory does not exist: {}\n  Run {} to configure it.",
            workspace_dir.display(),
            "buona config setup",
        );
    }

    let entries = fs::read_dir(&workspace_dir).with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    let mut workspaces: Vec<(String, WorkspaceMeta)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                let meta = read_meta(&entry.path())?;
                Some((dir_name, meta))
            } else {
                None
            }
        })
        .collect();

    workspaces.sort_by(|a, b| a.0.cmp(&b.0));

    if workspaces.is_empty() {
        println!(
            "  {}",
            s.dim.apply_to(format!(
                "No workspaces found in {}",
                workspace_dir.display()
            ))
        );
    } else {
        println!(
            "  {}  {}",
            s.dim.apply_to("Directory:"),
            workspace_dir.display()
        );
        println!();
        for (dir_name, meta) in &workspaces {
            if meta.name != *dir_name {
                println!(
                    "  {}  {} {}",
                    s.cyan.apply_to("•"),
                    meta.name,
                    s.dim.apply_to(format!("({dir_name})"))
                );
            } else {
                println!("  {}  {dir_name}", s.cyan.apply_to("•"));
            }
        }
    }

    println!();
    Ok(())
}

/// Create a new workspace directory. Writes a `buona.workspace.json` marker
/// file with the workspace name. If `name` is not provided, the directory name
/// is used.
pub fn create(path: &str, name: Option<&str>) -> Result<()> {
    let s = Styles::default();

    // Resolve the target directory
    let target: PathBuf = if PathBuf::from(path).is_absolute() {
        PathBuf::from(path)
    } else {
        config::workspace_dir()?.join(path)
    };

    // Derive the workspace name
    let ws_name = match name {
        Some(n) => n.to_string(),
        None => target
            .file_name()
            .context("could not determine directory name from path")?
            .to_string_lossy()
            .into_owned(),
    };

    if target.exists() {
        bail!("directory already exists: {}", target.display());
    }

    // Create the workspace directory (and any parent directories)
    fs::create_dir_all(&target).with_context(|| {
        format!(
            "could not create workspace directory: {}",
            target.display()
        )
    })?;

    // Write the workspace metadata file
    let meta = WorkspaceMeta {
        name: ws_name,
        packages: Vec::new(),
    };
    write_meta(&target, &meta)?;

    println!();
    println!(
        "  {} Created workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&meta.name)
    );
    println!(
        "  {}  {}",
        s.dim.apply_to("Location:"),
        target.display()
    );
    println!();

    Ok(())
}

/// Remove a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub fn remove(query: &str, force: bool) -> Result<()> {
    let s = Styles::default();

    let target = find_workspace(query)?;

    let meta = read_meta(&target);
    let display_name = meta
        .as_ref()
        .map(|m| m.name.as_str())
        .unwrap_or(query);

    if !force {
        println!();
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "  Remove workspace {} at {}?",
                s.bold.apply_to(display_name),
                s.dim.apply_to(target.display().to_string())
            ))
            .default(false)
            .interact()
            .context("failed to read input")?;

        if !confirmed {
            println!("  Aborted.");
            println!();
            return Ok(());
        }
    }

    fs::remove_dir_all(&target).with_context(|| {
        format!(
            "could not remove workspace directory: {}",
            target.display()
        )
    })?;

    println!();
    println!(
        "  {} Removed workspace {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(display_name)
    );
    println!();

    Ok(())
}

/// Add one or more packages to a workspace by cloning them into `src/`.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub fn add(packages: &[String], workspace: Option<&str>) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let mut meta = read_meta(&ws_root).context(
        "could not read workspace metadata — is this a valid buona workspace?",
    )?;

    let src_dir = ws_root.join("src");

    println!();
    println!(
        "  {} Adding packages to {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    let mut successes: Vec<PackageEntry> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for spec in packages {
        let resolved = match resolve_package_spec(spec, &cfg.git) {
            Ok(r) => r,
            Err(e) => {
                failures.push((spec.clone(), format!("{e}")));
                println!(
                    "  {} {} — {}",
                    s.red.apply_to("✘"),
                    spec,
                    e
                );
                continue;
            }
        };

        let dest = src_dir.join(&resolved.name);
        if dest.exists() {
            let msg = format!("directory already exists: {}", dest.display());
            failures.push((spec.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
            continue;
        }

        // Ensure src/ directory exists
        fs::create_dir_all(&src_dir).with_context(|| {
            format!("could not create src directory: {}", src_dir.display())
        })?;

        println!(
            "  {} Cloning {} ...",
            s.dim.apply_to("→"),
            s.cyan.apply_to(&resolved.name)
        );

        let output = Command::new("git")
            .arg("clone")
            .arg(&resolved.url)
            .arg(&dest)
            .output()
            .context("failed to execute git clone — is git installed?")?;

        if output.status.success() {
            println!(
                "  {} {}",
                s.green.apply_to("✔"),
                s.bold.apply_to(&resolved.name)
            );
            successes.push(PackageEntry {
                name: resolved.name,
                url: resolved.url,
            });
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            failures.push((spec.clone(), msg.clone()));
            println!(
                "  {} {} — {}",
                s.red.apply_to("✘"),
                spec,
                msg
            );
        }
    }

    // Update workspace metadata with successfully cloned packages
    if !successes.is_empty() {
        meta.packages.extend(successes.iter().cloned());
        write_meta(&ws_root, &meta)?;
    }

    // Print summary
    println!();
    if !failures.is_empty() {
        println!(
            "  {} added, {} failed",
            successes.len(),
            failures.len()
        );
    } else {
        println!(
            "  {} {} package{} added",
            s.green.apply_to("✔"),
            successes.len(),
            if successes.len() == 1 { "" } else { "s" }
        );
    }
    println!();

    if !failures.is_empty() && successes.is_empty() {
        bail!("all packages failed to add");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn workspace_meta_round_trips_through_serde() {
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-project");
        assert!(deserialized.packages.is_empty());
    }

    #[test]
    fn workspace_meta_with_packages_round_trips() {
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            packages: vec![
                PackageEntry {
                    name: "toolkit".to_string(),
                    url: "git@github.com:acme/toolkit.git".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let deserialized: WorkspaceMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.packages.len(), 1);
        assert_eq!(deserialized.packages[0].name, "toolkit");
        assert_eq!(deserialized.packages[0].url, "git@github.com:acme/toolkit.git");
    }

    #[test]
    fn workspace_meta_without_packages_field_defaults_to_empty() {
        let json = r#"{"name": "old-workspace"}"#;
        let meta: WorkspaceMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.name, "old-workspace");
        assert!(meta.packages.is_empty());
    }

    #[test]
    fn workspace_meta_empty_packages_not_serialized() {
        let meta = WorkspaceMeta {
            name: "clean".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("packages"));
    }

    #[test]
    fn read_meta_returns_some_for_valid_workspace() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test-workspace".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = read_meta(dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "test-workspace");
    }

    #[test]
    fn read_meta_returns_none_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = read_meta(dir.path());
        assert!(result.is_none());
    }

    // ── extract_package_name tests ───────────────────────────────────

    #[test]
    fn extract_name_from_https_url() {
        assert_eq!(
            extract_package_name("https://github.com/acme/toolkit.git"),
            "toolkit"
        );
    }

    #[test]
    fn extract_name_from_https_url_no_dot_git() {
        assert_eq!(
            extract_package_name("https://github.com/acme/toolkit"),
            "toolkit"
        );
    }

    #[test]
    fn extract_name_from_ssh_url() {
        assert_eq!(
            extract_package_name("git@github.com:acme/toolkit.git"),
            "toolkit"
        );
    }

    #[test]
    fn extract_name_from_ssh_url_no_slash() {
        assert_eq!(
            extract_package_name("git@github.com:toolkit.git"),
            "toolkit"
        );
    }

    #[test]
    fn extract_name_from_plain_name() {
        assert_eq!(extract_package_name("toolkit"), "toolkit");
    }

    // ── build_clone_url tests ────────────────────────────────────────

    #[test]
    fn build_clone_url_ssh() {
        assert_eq!(
            build_clone_url("github.com", "acme", "toolkit", GitProtocol::Ssh),
            "git@github.com:acme/toolkit.git"
        );
    }

    #[test]
    fn build_clone_url_https() {
        assert_eq!(
            build_clone_url("github.com", "acme", "toolkit", GitProtocol::Https),
            "https://github.com/acme/toolkit.git"
        );
    }

    // ── resolve_package_spec tests ───────────────────────────────────

    fn test_git_config() -> config::GitConfig {
        config::GitConfig {
            host: "github.com".to_string(),
            organization: "myorg".to_string(),
            protocol: GitProtocol::Ssh,
        }
    }

    #[test]
    fn resolve_direct_package_name_ssh() {
        let git = test_git_config();
        let result = resolve_package_spec("toolkit", &git).unwrap();
        assert_eq!(result.url, "git@github.com:myorg/toolkit.git");
        assert_eq!(result.name, "toolkit");
    }

    #[test]
    fn resolve_direct_package_name_https() {
        let mut git = test_git_config();
        git.protocol = GitProtocol::Https;
        let result = resolve_package_spec("toolkit", &git).unwrap();
        assert_eq!(result.url, "https://github.com/myorg/toolkit.git");
        assert_eq!(result.name, "toolkit");
    }

    #[test]
    fn resolve_direct_package_name_fails_without_org() {
        let mut git = test_git_config();
        git.organization = String::new();
        let result = resolve_package_spec("toolkit", &git);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_org_package_ssh() {
        let git = test_git_config();
        let result = resolve_package_spec("acme/toolkit", &git).unwrap();
        assert_eq!(result.url, "git@github.com:acme/toolkit.git");
        assert_eq!(result.name, "toolkit");
    }

    #[test]
    fn resolve_org_package_https() {
        let mut git = test_git_config();
        git.protocol = GitProtocol::Https;
        let result = resolve_package_spec("acme/toolkit", &git).unwrap();
        assert_eq!(result.url, "https://github.com/acme/toolkit.git");
        assert_eq!(result.name, "toolkit");
    }

    #[test]
    fn resolve_full_https_url() {
        let git = test_git_config();
        let result =
            resolve_package_spec("https://github.com/other/repo.git", &git).unwrap();
        assert_eq!(result.url, "https://github.com/other/repo.git");
        assert_eq!(result.name, "repo");
    }

    #[test]
    fn resolve_full_ssh_url() {
        let git = test_git_config();
        let result =
            resolve_package_spec("git@github.com:other/repo.git", &git).unwrap();
        assert_eq!(result.url, "git@github.com:other/repo.git");
        assert_eq!(result.name, "repo");
    }

    #[test]
    fn resolve_full_url_ignores_config_org() {
        let mut git = test_git_config();
        git.organization = String::new(); // no org configured
        let result =
            resolve_package_spec("https://github.com/other/repo.git", &git).unwrap();
        assert_eq!(result.url, "https://github.com/other/repo.git");
        assert_eq!(result.name, "repo");
    }

    // ── find_workspace_root tests ────────────────────────────────────

    #[test]
    fn find_workspace_root_in_workspace_dir() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        let result = find_workspace_root(dir.path()).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_workspace_root_in_child_dir() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(dir.path().join(WORKSPACE_FILE), json).unwrap();

        // Create a child directory and search from there
        let child = dir.path().join("src").join("deep");
        fs::create_dir_all(&child).unwrap();

        let result = find_workspace_root(&child).unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn find_workspace_root_fails_when_not_in_workspace() {
        let dir = TempDir::new().unwrap();
        let result = find_workspace_root(dir.path());
        assert!(result.is_err());
    }

    // ── write_meta tests ─────────────────────────────────────────────

    #[test]
    fn write_meta_creates_file() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "test".to_string(),
            packages: vec![PackageEntry {
                name: "pkg".to_string(),
                url: "git@github.com:org/pkg.git".to_string(),
            }],
        };
        write_meta(dir.path(), &meta).unwrap();

        let result = read_meta(dir.path()).unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].name, "pkg");
    }
}
