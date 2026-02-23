# Workspace Templates & Default Packages — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add workspace template directory, default packages, and auto-run install to `buona workspace create`.

**Architecture:** Extend `BuonaConfig` with two new optional fields (`default_packages`, `workspace_template`). Modify the `workspace::create()` function to: (1) copy template files into new workspace, (2) merge default packages with explicit packages, (3) auto-run `buona run install` after packages are added. Add CLI flags for opt-out.

**Tech Stack:** Rust, clap (CLI parsing), tokio (async I/O), serde (config serialization)

---

### Task 1: Add `default_packages` field to BuonaConfig

**Files:**
- Modify: `src/config.rs:138-159` (BuonaConfig struct and Default impl)

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `src/config.rs`:

```rust
#[test]
fn config_without_default_packages_gets_empty_vec() {
    let json = r#"{"workspace_dir": "~/workspace"}"#;
    let config: BuonaConfig = serde_json::from_str(json).unwrap();
    assert!(config.default_packages.is_empty());
}

#[test]
fn config_with_default_packages_round_trips() {
    let config = BuonaConfig {
        workspace_dir: "~/workspace".to_string(),
        ide: Ide::default(),
        git: GitConfig::default(),
        default_packages: vec!["shippo-ai-tools".to_string()],
        workspace_template: None,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.default_packages, vec!["shippo-ai-tools"]);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test config::tests::config_without_default_packages -- --exact`
Expected: compilation error — `default_packages` field doesn't exist

**Step 3: Add the fields to BuonaConfig**

In `src/config.rs`, modify the `BuonaConfig` struct (line 138-149):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BuonaConfig {
    pub(crate) workspace_dir: String,

    /// The user's preferred IDE.
    #[serde(default)]
    pub(crate) ide: Ide,

    /// Default git settings.
    #[serde(default)]
    pub(crate) git: GitConfig,

    /// Packages to include in every new workspace by default.
    #[serde(default)]
    pub(crate) default_packages: Vec<String>,

    /// Path to a template directory whose contents are copied into new workspaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_template: Option<String>,
}
```

Update `Default` impl (line 151-159):

```rust
impl Default for BuonaConfig {
    fn default() -> Self {
        Self {
            workspace_dir: "~/workspace".to_string(),
            ide: Ide::default(),
            git: GitConfig::default(),
            default_packages: Vec::new(),
            workspace_template: None,
        }
    }
}
```

Update `run_setup()` in `src/config.rs` (line 363-372) — add the new fields to the constructed config:

```rust
let config = BuonaConfig {
    workspace_dir,
    ide,
    git: GitConfig {
        host: git_host,
        organization: git_org,
        protocol: git_protocol,
        tracking: git_tracking,
    },
    default_packages: current.default_packages.clone(),
    workspace_template: current.workspace_template.clone(),
};
```

Update `print_pretty()` in `src/config.rs` — add after the Git Defaults section (before the file_exists check around line 258):

```rust
if !config.default_packages.is_empty() {
    println!();
    println!("  {}", s.bold.apply_to("Workspace Defaults"));
    println!("  {}", s.dim.apply_to("───────────────────"));
    println!(
        "  {}  {}",
        s.cyan.apply_to("Default Packages:"),
        config.default_packages.join(", ")
    );
}
if let Some(ref tmpl) = config.workspace_template {
    if config.default_packages.is_empty() {
        println!();
        println!("  {}", s.bold.apply_to("Workspace Defaults"));
        println!("  {}", s.dim.apply_to("───────────────────"));
    }
    println!(
        "  {}  {}",
        s.cyan.apply_to("Workspace Template:"),
        tmpl
    );
}
```

Also fix any existing tests that construct `BuonaConfig` directly — they need the new fields. Search for `BuonaConfig {` in `src/config.rs` tests and add `default_packages: Vec::new(), workspace_template: None,` to each.

**Step 4: Run all tests**

Run: `cargo test config::tests`
Expected: all PASS

**Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add default_packages and workspace_template to config"
```

---

### Task 2: Add CLI flags to workspace create command

**Files:**
- Modify: `src/main.rs:148-168` (WorkspaceCommands::Create variant)
- Modify: `src/main.rs:353-368` (match arm that calls workspace::create)

**Step 1: Add flags to the Create variant**

In `src/main.rs`, expand the `Create` variant (around line 149-168):

```rust
/// Create a new workspace
Create {
    /// Path for the new workspace (relative to the configured workspace directory, or absolute)
    path: String,

    /// Optional display name for the workspace (defaults to the directory name)
    #[arg(short, long)]
    name: Option<String>,

    /// Package specifier(s) to add after creation: name, org/name, or a full git URL
    #[arg(short = 'p', long = "package")]
    packages: Option<Vec<String>>,

    /// Open the workspace in the configured editor after creation
    #[arg(long)]
    open: bool,

    /// Git tracking mode for this workspace (overrides global default)
    #[arg(long, value_enum)]
    git_tracking: Option<config::GitTracking>,

    /// Skip default packages from global config
    #[arg(long)]
    no_defaults: bool,

    /// Path to a template directory (overrides global config workspace_template)
    #[arg(long)]
    template: Option<String>,

    /// Skip workspace template
    #[arg(long)]
    no_template: bool,

    /// Skip auto-running install after adding packages
    #[arg(long)]
    no_install: bool,
},
```

**Step 2: Update the match arm to pass new flags**

In the `WorkspaceCommands::Create` match arm (line 353-368):

```rust
WorkspaceCommands::Create {
    path,
    name,
    packages,
    open,
    git_tracking,
    no_defaults,
    template,
    no_template,
    no_install,
} => {
    workspace::create(
        Path::new(&path),
        name.as_deref(),
        packages.as_deref(),
        open,
        git_tracking,
        no_defaults,
        template.as_deref(),
        no_template,
        no_install,
    )
    .await?;
}
```

**Step 3: Update workspace::create signature**

This will break compilation until Task 3 is done. Update the function signature in `src/workspace/ops.rs`:

```rust
pub(crate) async fn create(
    path: &Path,
    name: Option<&str>,
    packages: Option<&[String]>,
    open_ws: bool,
    git_tracking: Option<GitTracking>,
    no_defaults: bool,
    template_override: Option<&str>,
    no_template: bool,
    no_install: bool,
) -> Result<()> {
```

Also update the re-export in `src/workspace/mod.rs` if needed (it re-exports `create` from `ops`).

**Step 4: Verify compilation**

Run: `cargo check`
Expected: may fail if Task 3 body changes aren't done yet — that's fine, this task establishes the API.

**Step 5: Commit**

```bash
git add src/main.rs src/workspace/ops.rs src/workspace/mod.rs
git commit -m "feat: add template and default-packages CLI flags to workspace create"
```

---

### Task 3: Implement workspace template copying

**Files:**
- Modify: `src/workspace/ops.rs` (inside `create()` function)
- New module: `src/workspace/template.rs`

**Step 1: Write failing test for template copying**

Create `src/workspace/template.rs` with test:

```rust
//! Workspace template — copies files from a template directory into a new workspace.

use std::path::Path;

use anyhow::{Context, Result};

/// Copy the contents of `template_dir` into `target`, preserving directory
/// structure and file permissions. Skips `buona.workspace.json` and any
/// `.code-workspace` file to avoid overwriting workspace metadata.
pub(super) async fn apply_template(template_dir: &Path, target: &Path) -> Result<()> {
    copy_dir_recursive(template_dir, target).await
}

async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    let mut entries = tokio::fs::read_dir(src)
        .await
        .with_context(|| format!("could not read template directory: {}", src.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        // Skip workspace metadata files
        if name == "buona.workspace.json" || name.ends_with(".code-workspace") {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let file_type = entry.file_type().await?;

        if file_type.is_dir() {
            tokio::fs::create_dir_all(&dst_path).await.with_context(|| {
                format!("could not create directory: {}", dst_path.display())
            })?;
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await.with_context(|| {
                format!(
                    "could not copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;

            // Preserve permissions (important for executable hooks)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = tokio::fs::metadata(&src_path).await?;
                let perms = metadata.permissions();
                tokio::fs::set_permissions(&dst_path, perms).await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn apply_template_copies_files() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        // Create template files
        tokio::fs::write(template.path().join("CLAUDE.md"), "# Test").await.unwrap();
        tokio::fs::write(template.path().join("buona.json"), "{}").await.unwrap();

        apply_template(template.path(), target.path()).await.unwrap();

        assert!(target.path().join("CLAUDE.md").exists());
        assert!(target.path().join("buona.json").exists());
    }

    #[tokio::test]
    async fn apply_template_copies_nested_directories() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let hooks_dir = template.path().join(".buona").join("hooks");
        tokio::fs::create_dir_all(&hooks_dir).await.unwrap();
        tokio::fs::write(hooks_dir.join("postinstall"), "#!/bin/sh\necho hi").await.unwrap();

        apply_template(template.path(), target.path()).await.unwrap();

        let copied = target.path().join(".buona").join("hooks").join("postinstall");
        assert!(copied.exists());
        let content = tokio::fs::read_to_string(&copied).await.unwrap();
        assert_eq!(content, "#!/bin/sh\necho hi");
    }

    #[tokio::test]
    async fn apply_template_skips_workspace_metadata() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        // These should be skipped
        tokio::fs::write(template.path().join("buona.workspace.json"), "{}").await.unwrap();
        tokio::fs::write(template.path().join("test.code-workspace"), "{}").await.unwrap();
        // This should be copied
        tokio::fs::write(template.path().join("CLAUDE.md"), "# Test").await.unwrap();

        apply_template(template.path(), target.path()).await.unwrap();

        assert!(!target.path().join("buona.workspace.json").exists());
        assert!(!target.path().join("test.code-workspace").exists());
        assert!(target.path().join("CLAUDE.md").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn apply_template_preserves_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let hook_path = template.path().join("hook.sh");
        tokio::fs::write(&hook_path, "#!/bin/sh").await.unwrap();
        tokio::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();

        apply_template(template.path(), target.path()).await.unwrap();

        let copied = target.path().join("hook.sh");
        let meta = tokio::fs::metadata(&copied).await.unwrap();
        let mode = meta.permissions().mode();
        assert!(mode & 0o111 != 0, "executable bit should be preserved");
    }

    #[tokio::test]
    async fn apply_template_nonexistent_dir_returns_error() {
        let target = TempDir::new().unwrap();
        let result = apply_template(Path::new("/tmp/nonexistent-template-dir-xyz"), target.path()).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Register the module**

In `src/workspace/mod.rs`, add:

```rust
mod template;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test workspace::template::tests`
Expected: all PASS

**Step 4: Commit**

```bash
git add src/workspace/template.rs src/workspace/mod.rs
git commit -m "feat: add workspace template copy module"
```

---

### Task 4: Wire template + default packages + auto-install into create()

**Files:**
- Modify: `src/workspace/ops.rs:117-213` (the `create()` function body)

**Step 1: Write integration-style tests**

Add to `src/workspace/ops.rs` tests:

```rust
#[tokio::test]
async fn create_applies_template_when_configured() {
    let workspace_dir = TempDir::new().unwrap();
    let template_dir = TempDir::new().unwrap();

    // Set up template with a file
    tokio::fs::write(template_dir.path().join("CLAUDE.md"), "# Template").await.unwrap();

    let target = workspace_dir.path().join("test-ws");

    // Call create with template override
    create(
        &target,
        None,
        None,
        false,
        None,
        false,
        Some(template_dir.path().to_str().unwrap()),
        false,
        true, // no_install — skip install since we have no packages
    )
    .await
    .unwrap();

    assert!(target.join("CLAUDE.md").exists());
    assert!(target.join("buona.workspace.json").exists());
    assert!(target.join("src").exists());
}

#[tokio::test]
async fn create_skips_template_when_no_template_flag() {
    let workspace_dir = TempDir::new().unwrap();
    let target = workspace_dir.path().join("test-ws");

    create(
        &target,
        None,
        None,
        false,
        None,
        false,
        Some("/some/template"),
        true, // no_template
        true,
    )
    .await
    .unwrap();

    // Only standard files, no template applied
    assert!(target.join("buona.workspace.json").exists());
    assert!(!target.join("CLAUDE.md").exists());
}
```

**Step 2: Implement the updated create() function body**

Replace the body of `create()` in `src/workspace/ops.rs`. Key changes:

After writing `buona.workspace.json` and before syncing workspace file, add template logic:

```rust
// Apply workspace template if configured
if !no_template {
    let template_path = match template_override {
        Some(p) => Some(config::expand_tilde(p)?),
        None => match cfg.workspace_template {
            Some(ref p) => Some(config::expand_tilde(p)?),
            None => None,
        },
    };

    if let Some(ref tmpl) = template_path {
        if tmpl.is_dir() {
            super::template::apply_template(tmpl, &target).await?;
            println!(
                "  {} Applied workspace template",
                s.green.apply_to("✔"),
            );
        } else {
            eprintln!(
                "  {} Workspace template directory not found: {}",
                s.yellow.apply_to("⚠"),
                tmpl.display()
            );
        }
    }
}
```

For default packages, after the workspace creation messages, modify the package-adding block:

```rust
// Merge default packages with explicit packages
let mut all_packages: Vec<String> = Vec::new();

if !no_defaults {
    for pkg in &cfg.default_packages {
        all_packages.push(pkg.clone());
    }
}

if let Some(pkgs) = packages {
    for pkg in pkgs {
        if !all_packages.contains(pkg) {
            all_packages.push(pkg.clone());
        }
    }
}

let packages_added = !all_packages.is_empty();
if packages_added {
    add_packages_to_workspace(&target, &all_packages).await?;
}
```

For auto-install, after packages are added:

```rust
// Auto-run install if packages were added and buona.json exists
if packages_added && !no_install && target.join("buona.json").exists() {
    println!();
    println!("  {} Running install...", s.cyan.apply_to("→"));

    let status = tokio::process::Command::new(std::env::current_exe()?)
        .args(["run", "install"])
        .current_dir(&target)
        .status()
        .await
        .context("failed to run install")?;

    if !status.success() {
        eprintln!(
            "  {} Install finished with non-zero exit code",
            s.yellow.apply_to("⚠"),
        );
    }
}
```

Note: `cfg` is already loaded on line 171 of the current code. Move that load earlier (before template logic) and reuse it.

**Step 3: Run tests**

Run: `cargo test workspace::ops::tests`
Expected: all PASS (existing + new tests)

**Step 4: Run full test suite**

Run: `cargo test`
Expected: all 239+ tests PASS

**Step 5: Commit**

```bash
git add src/workspace/ops.rs
git commit -m "feat: wire template, default packages, and auto-install into workspace create"
```

---

### Task 5: Update print_pretty and run_setup for new config fields

**Files:**
- Modify: `src/config.rs` (print_pretty + run_setup)

This was partially covered in Task 1. Verify the print_pretty additions display correctly and run_setup preserves existing values for the new fields.

**Step 1: Write a test for config round-trip with all fields**

```rust
#[test]
fn config_with_all_new_fields_round_trips() {
    let config = BuonaConfig {
        workspace_dir: "~/workspace".to_string(),
        ide: Ide::default(),
        git: GitConfig::default(),
        default_packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
        workspace_template: Some("~/.config/buona/workspace-template".to_string()),
    };
    let json = serde_json::to_string_pretty(&config).unwrap();
    let deserialized: BuonaConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.default_packages, config.default_packages);
    assert_eq!(deserialized.workspace_template, config.workspace_template);
}

#[test]
fn config_without_workspace_template_omits_field() {
    let config = BuonaConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("workspace_template"));
}
```

**Step 2: Run tests**

Run: `cargo test config::tests`
Expected: all PASS

**Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: display new workspace config fields in print_pretty"
```

---

### Task 6: Manual end-to-end validation

**Step 1: Build buona**

Run: `cargo build`

**Step 2: Set up global config**

```bash
./target/debug/buona config set default_packages '["shippo-ai-tools"]' --json
./target/debug/buona config set workspace_template '"~/.config/buona/workspace-template"' --json
./target/debug/buona config show
```

Verify the new fields appear in output.

**Step 3: Create template directory**

```bash
mkdir -p ~/.config/buona/workspace-template/.buona/hooks
```

Copy the working files from `~/workspace/shippo-test/`:
- `buona.json` → `~/.config/buona/workspace-template/buona.json`
- `.buona/hooks/postinstall` → `~/.config/buona/workspace-template/.buona/hooks/postinstall`
- `CLAUDE.md` → `~/.config/buona/workspace-template/CLAUDE.md`

**Step 4: Create a test workspace**

```bash
./target/debug/buona workspace create test-template-ws
```

Verify:
- Template files copied (buona.json, .buona/hooks/postinstall, CLAUDE.md)
- shippo-ai-tools cloned into src/
- Install ran automatically
- `.claude/` dirs appear at workspace root

**Step 5: Test opt-out flags**

```bash
./target/debug/buona workspace create test-no-defaults --no-defaults --no-template
```

Verify: no template files, no default packages, just bare workspace.

**Step 6: Clean up test workspaces**

```bash
./target/debug/buona workspace delete test-template-ws --force
./target/debug/buona workspace delete test-no-defaults --force
```

**Step 7: Commit any fixes discovered during validation**

```bash
git add -A
git commit -m "fix: address issues found during manual validation"
```

---

### Task 7: Final review and PR

**Step 1: Run full test suite one more time**

Run: `cargo test`
Expected: all PASS

**Step 2: Review all changes**

Run: `git diff main --stat` to see all changed files.

**Step 3: Create a feature branch and PR**

```bash
git checkout -b feat/workspace-templates
git push -u origin feat/workspace-templates
```

Create PR with summary of the three features.
