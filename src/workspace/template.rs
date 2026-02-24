//! Workspace template — copies files from a template directory into a new workspace.

use std::path::Path;

use anyhow::{Context, Result};

/// Copy the contents of `template_dir` into `target`, preserving directory
/// structure and file permissions. Skips `buona.workspace.json` and any
/// `.code-workspace` file to avoid overwriting workspace metadata.
pub(super) async fn apply_template(template_dir: &Path, target: &Path) -> Result<()> {
    copy_dir_recursive(template_dir, target).await
}

fn copy_dir_recursive<'a>(
    src: &'a Path,
    dst: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(copy_dir_recursive_inner(src, dst))
}

async fn copy_dir_recursive_inner(src: &Path, dst: &Path) -> Result<()> {
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
            tokio::fs::create_dir_all(&dst_path)
                .await
                .with_context(|| format!("could not create directory: {}", dst_path.display()))?;
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path)
                .await
                .with_context(|| {
                    format!(
                        "could not copy {} to {}",
                        src_path.display(),
                        dst_path.display()
                    )
                })?;

            // Preserve permissions (important for executable hooks).
            // tokio::fs::copy on unix preserves permissions via the underlying
            // std::fs::copy, but we re-apply explicitly to be defensive.
            #[cfg(unix)]
            {
                let metadata = tokio::fs::metadata(&src_path).await?;
                tokio::fs::set_permissions(&dst_path, metadata.permissions()).await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn apply_template_copies_files() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        fs::write(template.path().join("CLAUDE.md"), "# Claude")
            .await
            .unwrap();
        fs::write(template.path().join("buona.json"), "{}")
            .await
            .unwrap();

        apply_template(template.path(), target.path())
            .await
            .unwrap();

        assert_eq!(
            fs::read_to_string(target.path().join("CLAUDE.md"))
                .await
                .unwrap(),
            "# Claude"
        );
        assert_eq!(
            fs::read_to_string(target.path().join("buona.json"))
                .await
                .unwrap(),
            "{}"
        );
    }

    #[tokio::test]
    async fn apply_template_copies_nested_directories() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let hooks_dir = template.path().join(".buona/hooks");
        fs::create_dir_all(&hooks_dir).await.unwrap();
        fs::write(hooks_dir.join("postinstall"), "#!/bin/sh\necho hi")
            .await
            .unwrap();

        apply_template(template.path(), target.path())
            .await
            .unwrap();

        let copied = target.path().join(".buona/hooks/postinstall");
        assert!(copied.exists());
        assert_eq!(
            fs::read_to_string(&copied).await.unwrap(),
            "#!/bin/sh\necho hi"
        );
    }

    #[tokio::test]
    async fn apply_template_skips_workspace_metadata() {
        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        fs::write(template.path().join("buona.workspace.json"), "{}")
            .await
            .unwrap();
        fs::write(template.path().join("test.code-workspace"), "{}")
            .await
            .unwrap();
        fs::write(template.path().join("keep.txt"), "keep")
            .await
            .unwrap();

        apply_template(template.path(), target.path())
            .await
            .unwrap();

        assert!(!target.path().join("buona.workspace.json").exists());
        assert!(!target.path().join("test.code-workspace").exists());
        assert!(target.path().join("keep.txt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn apply_template_preserves_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let template = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let script = template.path().join("run.sh");
        fs::write(&script, "#!/bin/sh").await.unwrap();
        fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();

        apply_template(template.path(), target.path())
            .await
            .unwrap();

        let copied = target.path().join("run.sh");
        let perms = fs::metadata(&copied).await.unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o755);
    }

    #[tokio::test]
    async fn apply_template_nonexistent_dir_returns_error() {
        let target = TempDir::new().unwrap();
        let bad_path = target.path().join("does-not-exist");

        let result = apply_template(&bad_path, target.path()).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("could not read template directory")
        );
    }
}
