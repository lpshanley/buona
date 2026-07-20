//! Package add/clone workflow helpers.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{BuonaConfig, GitTracking};
use crate::styles::Styles;

use super::git::resolve_package_spec;
use super::git_ops;
use super::types::read_meta;
use super::workspace_file::sync_workspace_file;

/// Add packages to a specific workspace root.
///
/// Internal helper that adds packages to `src/` without resolving workspace by name.
pub(super) async fn add_packages_to_workspace(
    ws_root: &Path,
    packages: &[String],
    cfg: &BuonaConfig,
) -> Result<()> {
    let s = Styles::default();

    let meta = read_meta(ws_root)
        .await?
        .context("could not read workspace metadata — is this a valid buona workspace?")?;

    let tracking = meta.effective_tracking(cfg);
    let src_dir = ws_root.join("src");

    println!();
    println!(
        "  {} Adding packages to {}",
        s.bold.apply_to("📦"),
        s.bold.apply_to(&meta.name)
    );
    println!("  {}", s.dim.apply_to("───────────────────────────"));

    let mut successes: Vec<String> = Vec::new();
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

        tokio::fs::create_dir_all(&src_dir)
            .await
            .with_context(|| format!("could not create src directory: {}", src_dir.display()))?;

        println!(
            "  {} Cloning {} ...",
            s.dim.apply_to("→"),
            s.cyan.apply_to(&resolved.name)
        );

        let output = git_ops::clone_into(&resolved.url, &dest).await?;

        if output.status.success() {
            if tracking == GitTracking::Workspace {
                let pkg_git_dir = dest.join(".git");
                if pkg_git_dir.exists() {
                    tokio::fs::remove_dir_all(&pkg_git_dir)
                        .await
                        .with_context(|| {
                            format!(
                                "could not remove .git directory from cloned package: {}",
                                pkg_git_dir.display()
                            )
                        })?;
                }
            }

            println!(
                "  {} {}",
                s.green.apply_to("✔"),
                s.bold.apply_to(&resolved.name)
            );
            successes.push(resolved.name);
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            failures.push((spec.clone(), msg.clone()));
            println!("  {} {} — {}", s.red.apply_to("✘"), spec, msg);
        }
    }

    if !successes.is_empty() {
        sync_workspace_file(ws_root, &meta).await?;
    }

    println!();
    if !failures.is_empty() {
        println!(
            "  {} Summary: {} succeeded, {} failed",
            s.dim.apply_to("→"),
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
