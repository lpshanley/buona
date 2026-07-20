//! Workspace template — copies files from a template directory into a new workspace.

use std::path::Path;

use anyhow::{Context, Result};

use crate::fsutil;

/// Copy the contents of `template_dir` into `target`, preserving directory
/// structure and file permissions. Skips `buona.workspace.json` and any
/// `.code-workspace` file to avoid overwriting workspace metadata.
pub(super) async fn apply_template(template_dir: &Path, target: &Path) -> Result<()> {
    fsutil::copy_dir_recursive(template_dir, target, |name| {
        name == "buona.workspace.json" || name.ends_with(".code-workspace")
    })
    .await
    .with_context(|| {
        format!(
            "could not read template directory: {}",
            template_dir.display()
        )
    })
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
