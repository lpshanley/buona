//! Shared filesystem helpers: atomic writes and recursive directory copies.

use std::path::Path;

use anyhow::{Context, Result};

/// Write `contents` to `path` atomically: write to a temp file in the same
/// directory, then rename over the target. A crash mid-write can never leave
/// a truncated file behind.
pub(crate) async fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path
        .parent()
        .with_context(|| format!("could not determine parent directory of {}", path.display()))?;
    let file_name = path
        .file_name()
        .with_context(|| format!("could not determine file name of {}", path.display()))?
        .to_string_lossy();

    let tmp_path = dir.join(format!(".{}.{}.tmp", file_name, std::process::id()));

    tokio::fs::write(&tmp_path, contents)
        .await
        .with_context(|| format!("could not write temp file: {}", tmp_path.display()))?;

    if let Err(e) = tokio::fs::rename(&tmp_path, path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e).with_context(|| format!("could not write file: {}", path.display()));
    }

    Ok(())
}

/// Copy the contents of `src` into `dst` recursively, preserving directory
/// structure, file permissions, and symlinks. Entries whose file name matches
/// `skip` are not copied (checked at every depth).
pub(crate) async fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    skip: fn(&str) -> bool,
) -> Result<()> {
    copy_dir_boxed(src, dst, skip).await
}

fn copy_dir_boxed<'a>(
    src: &'a Path,
    dst: &'a Path,
    skip: fn(&str) -> bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(copy_dir_inner(src, dst, skip))
}

async fn copy_dir_inner(src: &Path, dst: &Path, skip: fn(&str) -> bool) -> Result<()> {
    let mut entries = tokio::fs::read_dir(src)
        .await
        .with_context(|| format!("could not read directory: {}", src.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        if skip(&file_name.to_string_lossy()) {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&file_name);
        let file_type = entry.file_type().await?;

        if file_type.is_symlink() {
            let link = tokio::fs::read_link(&src_path)
                .await
                .with_context(|| format!("could not read symlink: {}", src_path.display()))?;
            #[cfg(unix)]
            tokio::fs::symlink(&link, &dst_path)
                .await
                .with_context(|| format!("could not create symlink: {}", dst_path.display()))?;
            #[cfg(not(unix))]
            anyhow::bail!(
                "cannot copy symlink {} on this platform",
                src_path.display()
            );
        } else if file_type.is_dir() {
            tokio::fs::create_dir_all(&dst_path)
                .await
                .with_context(|| format!("could not create directory: {}", dst_path.display()))?;
            copy_dir_boxed(&src_path, &dst_path, skip).await?;
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

    #[tokio::test]
    async fn write_atomic_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        write_atomic(&path, "{}\n").await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
    }

    #[tokio::test]
    async fn write_atomic_replaces_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        std::fs::write(&path, "old").unwrap();
        write_atomic(&path, "new").await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[tokio::test]
    async fn write_atomic_leaves_no_temp_files() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        write_atomic(&path, "data").await.unwrap();

        let count = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(count, 1, "only the target file should remain");
    }

    #[tokio::test]
    async fn copy_dir_copies_nested_structure() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::create_dir_all(src.path().join("a/b")).unwrap();
        std::fs::write(src.path().join("a/b/file.txt"), "deep").unwrap();
        std::fs::write(src.path().join("top.txt"), "top").unwrap();

        copy_dir_recursive(src.path(), dst.path(), |_| false)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(dst.path().join("a/b/file.txt")).unwrap(),
            "deep"
        );
        assert_eq!(
            std::fs::read_to_string(dst.path().join("top.txt")).unwrap(),
            "top"
        );
    }

    #[tokio::test]
    async fn copy_dir_respects_skip() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::write(src.path().join("keep.txt"), "keep").unwrap();
        std::fs::write(src.path().join("skip.txt"), "skip").unwrap();

        copy_dir_recursive(src.path(), dst.path(), |name| name == "skip.txt")
            .await
            .unwrap();

        assert!(dst.path().join("keep.txt").exists());
        assert!(!dst.path().join("skip.txt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn copy_dir_preserves_symlinks() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        std::fs::write(src.path().join("target.txt"), "real").unwrap();
        std::os::unix::fs::symlink("target.txt", src.path().join("link.txt")).unwrap();

        copy_dir_recursive(src.path(), dst.path(), |_| false)
            .await
            .unwrap();

        let copied = dst.path().join("link.txt");
        assert!(std::fs::symlink_metadata(&copied).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "real");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn copy_dir_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        let script = src.path().join("run.sh");
        std::fs::write(&script, "#!/bin/sh").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        copy_dir_recursive(src.path(), dst.path(), |_| false)
            .await
            .unwrap();

        let perms = std::fs::metadata(dst.path().join("run.sh"))
            .unwrap()
            .permissions();
        assert_eq!(perms.mode() & 0o777, 0o755);
    }
}
