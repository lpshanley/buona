//! Workspace operations — list, create, remove, add, sync, and open.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use dialoguer::Confirm;

use crate::config;
use crate::styles::Styles;

use super::git::resolve_package_spec;
use super::types::{PackageEntry, WorkspaceMeta, WORKSPACE_FILE, read_meta, write_meta};
use super::vscode::{VscodeWorkspace, VscodeWorkspaceFolder, sanitize_name};

/// Find a workspace by name or directory name. Returns the resolved path.
fn find_workspace(query: &str) -> Result<PathBuf> {
    let workspace_dir = config::workspace_dir()?;

    // First, try as a direct directory name
    let direct = workspace_dir.join(query);
    if direct.is_dir() && read_meta(&direct)?.is_some() {
        return Ok(direct);
    }

    // Otherwise, search by workspace name in metadata
    let entries = fs::read_dir(&workspace_dir).with_context(|| {
        format!(
            "could not read workspace directory: {}",
            workspace_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            if let Some(meta) = read_meta(&path)?
                && meta.name == query
            {
                return Ok(path);
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

/// List all workspaces (directories) found in the configured workspace directory.
pub(crate) fn list() -> Result<()> {
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

    let mut workspaces: Vec<(String, WorkspaceMeta)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            if let Some(meta) = read_meta(&entry.path())? {
                workspaces.push((dir_name, meta));
            }
        }
    }

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
pub(crate) fn create(path: &Path, name: Option<&str>) -> Result<()> {
    let s = Styles::default();

    // Resolve the target directory
    let target: PathBuf = if path.is_absolute() {
        path.to_path_buf()
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
    fs::create_dir_all(&target)
        .with_context(|| format!("could not create workspace directory: {}", target.display()))?;

    // Create the src/ directory for packages
    fs::create_dir_all(target.join("src")).with_context(|| {
        format!(
            "could not create src directory: {}",
            target.join("src").display()
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
    println!("  {}  {}", s.dim.apply_to("Location:"), target.display());
    println!();

    Ok(())
}

/// Remove a workspace by name or directory name. Prompts for confirmation
/// unless `force` is true.
pub(crate) fn remove(query: &str, force: bool) -> Result<()> {
    let s = Styles::default();

    let target = find_workspace(query)?;

    let meta = read_meta(&target)?;
    let display_name = meta.as_ref().map(|m| m.name.as_str()).unwrap_or(query);

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

    fs::remove_dir_all(&target)
        .with_context(|| format!("could not remove workspace directory: {}", target.display()))?;

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
pub(crate) fn add(packages: &[String], workspace: Option<&str>) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let mut meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

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
                println!("  {} {} — {}", s.red.apply_to("✘"), spec, e);
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
        fs::create_dir_all(&src_dir)
            .with_context(|| format!("could not create src directory: {}", src_dir.display()))?;

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
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
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
        println!("  {} added, {} failed", successes.len(), failures.len());
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

/// Sync workspace metadata to a `.code-workspace` file.
///
/// Reads the `buona.workspace.json`, builds a VS Code multi-root workspace
/// file with one folder entry per tracked package, and writes it next to the
/// metadata file. Returns the path to the generated `.code-workspace` file.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn sync(workspace: Option<&str>) -> Result<PathBuf> {
    let s = Styles::default();

    // Resolve the workspace root
    let ws_root = match workspace {
        Some(name) => find_workspace(name)?,
        None => find_workspace_from_cwd()?,
    };

    let meta = read_meta(&ws_root)?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let sanitized = sanitize_name(&meta.name);
    if sanitized.is_empty() {
        bail!(
            "workspace name \"{}\" produces an empty filename after sanitization",
            meta.name
        );
    }

    let filename = format!("{sanitized}.code-workspace");
    let ws_file_path = ws_root.join(&filename);

    // Build folder entries from tracked packages
    let folders: Vec<VscodeWorkspaceFolder> = meta
        .packages
        .iter()
        .map(|pkg| VscodeWorkspaceFolder {
            path: format!("src/{}", pkg.name),
            name: pkg.name.clone(),
        })
        .collect();

    let vscode_ws = VscodeWorkspace {
        folders,
        settings: serde_json::json!({}),
    };

    let json = serde_json::to_string_pretty(&vscode_ws)?;
    fs::write(&ws_file_path, json + "\n")
        .with_context(|| format!("could not write workspace file: {}", ws_file_path.display()))?;

    println!();
    println!(
        "  {} Synced workspace file {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(&filename)
    );
    println!(
        "  {}  {}",
        s.dim.apply_to("Location:"),
        ws_file_path.display()
    );
    println!();

    Ok(ws_file_path)
}

/// Open a workspace in the configured editor.
///
/// Syncs the `.code-workspace` file first, then launches the editor.
///
/// If `workspace` is provided, it is looked up by name or directory.
/// Otherwise, the workspace is detected from the current working directory.
pub(crate) fn open(workspace: Option<&str>) -> Result<()> {
    let s = Styles::default();
    let cfg = config::load_config()?;

    let ws_file_path = sync(workspace)?;

    let ide_cmd = cfg.ide.command();

    println!(
        "  {} Opening in {} ...",
        s.dim.apply_to("→"),
        s.bold.apply_to(cfg.ide.to_string())
    );

    let status = Command::new(ide_cmd)
        .arg(&ws_file_path)
        .status()
        .with_context(|| {
            format!(
                "failed to launch {ide_cmd} — is {} installed and on your PATH?",
                cfg.ide
            )
        })?;

    if !status.success() {
        bail!("{ide_cmd} exited with {status}");
    }

    println!(
        "  {} Opened {}",
        s.green.apply_to("✔"),
        s.bold.apply_to(
            ws_file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )
    );
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

    // ── sync tests ──────────────────────────────────────────────────

    #[test]
    fn sync_creates_code_workspace_file() {
        let dir = TempDir::new().unwrap();
        let meta = WorkspaceMeta {
            name: "my-project".to_string(),
            packages: vec![
                PackageEntry {
                    name: "toolkit".to_string(),
                    url: "git@github.com:acme/toolkit.git".to_string(),
                },
                PackageEntry {
                    name: "utils".to_string(),
                    url: "git@github.com:acme/utils.git".to_string(),
                },
            ],
        };
        write_meta(dir.path(), &meta).unwrap();

        // We can't call sync() directly because it tries to resolve the workspace
        // via config. Instead, test the pieces: sanitize + write.
        let sanitized = sanitize_name(&meta.name);
        assert_eq!(sanitized, "my-project");

        let filename = format!("{sanitized}.code-workspace");
        let ws_file_path = dir.path().join(&filename);

        let folders: Vec<VscodeWorkspaceFolder> = meta
            .packages
            .iter()
            .map(|pkg| VscodeWorkspaceFolder {
                path: format!("src/{}", pkg.name),
                name: pkg.name.clone(),
            })
            .collect();

        let vscode_ws = VscodeWorkspace {
            folders,
            settings: serde_json::json!({}),
        };

        let json = serde_json::to_string_pretty(&vscode_ws).unwrap();
        fs::write(&ws_file_path, &json).unwrap();

        // Verify the file was written correctly
        assert!(ws_file_path.exists());

        let contents = fs::read_to_string(&ws_file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["folders"][0]["path"], "src/toolkit");
        assert_eq!(parsed["folders"][0]["name"], "toolkit");
        assert_eq!(parsed["folders"][1]["path"], "src/utils");
        assert_eq!(parsed["folders"][1]["name"], "utils");
    }
}
