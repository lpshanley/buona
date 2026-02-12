//! VS Code / Cursor `.code-workspace` file generation.

use serde::Serialize;

/// A single folder entry in a `.code-workspace` file.
#[derive(Debug, Serialize)]
pub(super) struct VscodeWorkspaceFolder {
    pub(super) path: String,
    pub(super) name: String,
}

/// The top-level structure of a `.code-workspace` file.
#[derive(Debug, Serialize)]
pub(super) struct VscodeWorkspace {
    pub(super) folders: Vec<VscodeWorkspaceFolder>,
    pub(super) settings: serde_json::Value,
}

/// Sanitize a workspace name into a filename-safe and shell-safe string.
///
/// Replaces any character that is not alphanumeric, hyphen, underscore, or
/// period with a hyphen, collapses consecutive hyphens, and trims
/// leading/trailing hyphens.
pub(super) fn sanitize_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens
    let mut result = String::with_capacity(replaced.len());
    let mut prev_hyphen = false;
    for c in replaced.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens
    result.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_name tests ─────────────────────────────────────────

    #[test]
    fn sanitize_name_plain_name_unchanged() {
        assert_eq!(sanitize_name("my-project"), "my-project");
    }

    #[test]
    fn sanitize_name_with_underscores_and_dots() {
        assert_eq!(sanitize_name("my_project.v2"), "my_project.v2");
    }

    #[test]
    fn sanitize_name_replaces_spaces() {
        assert_eq!(sanitize_name("My Cool Project"), "My-Cool-Project");
    }

    #[test]
    fn sanitize_name_replaces_special_characters() {
        assert_eq!(sanitize_name("project@v1!#$%"), "project-v1");
    }

    #[test]
    fn sanitize_name_collapses_consecutive_hyphens() {
        assert_eq!(sanitize_name("a---b"), "a-b");
    }

    #[test]
    fn sanitize_name_trims_leading_trailing_hyphens() {
        assert_eq!(sanitize_name("--project--"), "project");
    }

    #[test]
    fn sanitize_name_complex_input() {
        assert_eq!(sanitize_name("My Cool Project!"), "My-Cool-Project");
    }

    // ── VscodeWorkspace serialization tests ──────────────────────────

    #[test]
    fn vscode_workspace_serializes_correctly() {
        let ws = VscodeWorkspace {
            folders: vec![
                VscodeWorkspaceFolder {
                    path: "src/toolkit".to_string(),
                    name: "toolkit".to_string(),
                },
                VscodeWorkspaceFolder {
                    path: "src/utils".to_string(),
                    name: "utils".to_string(),
                },
            ],
            settings: serde_json::json!({}),
        };

        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&ws).unwrap()).unwrap();

        assert!(json["folders"].is_array());
        assert_eq!(json["folders"].as_array().unwrap().len(), 2);
        assert_eq!(json["folders"][0]["path"], "src/toolkit");
        assert_eq!(json["folders"][0]["name"], "toolkit");
        assert_eq!(json["folders"][1]["path"], "src/utils");
        assert_eq!(json["folders"][1]["name"], "utils");
        assert_eq!(json["settings"], serde_json::json!({}));
    }

    #[test]
    fn vscode_workspace_empty_folders() {
        let ws = VscodeWorkspace {
            folders: vec![],
            settings: serde_json::json!({}),
        };

        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&ws).unwrap()).unwrap();

        assert!(json["folders"].as_array().unwrap().is_empty());
    }

    // ── sync-related tests ──────────────────────────────────────────

    #[test]
    fn sync_sanitizes_workspace_name_for_filename() {
        let sanitized = sanitize_name("My Cool Project!");
        let filename = format!("{sanitized}.code-workspace");
        assert_eq!(filename, "My-Cool-Project.code-workspace");
    }
}
