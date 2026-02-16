//! Git URL resolution and package specifier parsing.

use anyhow::{Result, bail};

use crate::config;
use crate::config::GitProtocol;

/// Resolved package information: the git clone URL and the local directory name.
pub(super) struct ResolvedPackage {
    pub(super) url: String,
    pub(super) name: String,
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
pub(super) fn resolve_package_spec(spec: &str, git: &config::GitConfig) -> Result<ResolvedPackage> {
    let url = if spec.contains("://") || spec.starts_with("git@") {
        // Fully qualified URL — use as-is
        spec.to_string()
    } else if spec.contains('/') {
        // Org/Package pattern
        let (org, package) = spec.split_once('/').expect("already checked contains '/'");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitTracking;

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
            tracking: GitTracking::default(),
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
        let result = resolve_package_spec("https://github.com/other/repo.git", &git).unwrap();
        assert_eq!(result.url, "https://github.com/other/repo.git");
        assert_eq!(result.name, "repo");
    }

    #[test]
    fn resolve_full_ssh_url() {
        let git = test_git_config();
        let result = resolve_package_spec("git@github.com:other/repo.git", &git).unwrap();
        assert_eq!(result.url, "git@github.com:other/repo.git");
        assert_eq!(result.name, "repo");
    }

    #[test]
    fn resolve_full_url_ignores_config_org() {
        let mut git = test_git_config();
        git.organization = String::new(); // no org configured
        let result = resolve_package_spec("https://github.com/other/repo.git", &git).unwrap();
        assert_eq!(result.url, "https://github.com/other/repo.git");
        assert_eq!(result.name, "repo");
    }
}
