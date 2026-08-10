use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buona"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

fn run_with_home(directory: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buona"))
        .current_dir(directory)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .args(args)
        .output()
        .unwrap()
}

fn write_isolated_config(home: &Path, document: &Value) {
    let paths = [
        home.join(".config/buona/config.json"),
        home.join("Library/Application Support/buona/config.json"),
    ];
    for path in paths {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string_pretty(document).unwrap()).unwrap();
    }
}

fn parse_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!("invalid JSON ({error}): {}", String::from_utf8_lossy(bytes))
    })
}

#[test]
fn detect_json_matches_golden_document() {
    let fixture = root().join("tests/fixtures/systems/cargo");
    let output = run_in(&fixture, &["detect", "--output", "json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let mut actual = parse_json(&output.stdout);
    actual["targets"][0]["target"]["directory"] = Value::String("$FIXTURE".to_string());
    let expected = parse_json(&fs::read(root().join("tests/golden/detect-cargo.json")).unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn run_json_without_dry_run_is_a_structured_error() {
    let fixture = root().join("tests/fixtures/systems/cargo");
    let output = run_in(&fixture, &["run", "test", "--output", "json"]);
    assert_eq!(output.status.code(), Some(68));
    assert!(output.stdout.is_empty());

    let actual = parse_json(&output.stderr);
    let expected =
        parse_json(&fs::read(root().join("tests/golden/run-json-required-error.json")).unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn dry_run_json_contains_resolved_plan_without_text_noise() {
    let fixture = root().join("tests/fixtures/systems/cargo");
    let output = run_in(&fixture, &["run", "test", "--dry-run", "--output", "json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let document = parse_json(&output.stdout);
    assert_eq!(document["command"], "test");
    assert_eq!(document["targets"][0]["plan"]["program"], "cargo");
    assert_eq!(document["targets"][0]["plan"]["args"][0], "test");
}

#[test]
fn inspect_json_exposes_discovery_and_all_standard_commands() {
    let fixture = root().join("tests/fixtures/systems/cargo");
    let output = run_in(&fixture, &["inspect", "--output", "json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let document = parse_json(&output.stdout);
    assert_eq!(document["target"]["name"], "cargo");
    assert_eq!(document["detected_systems"][0]["system"], "cargo");
    assert_eq!(document["commands"]["test"]["plan"]["program"], "cargo");
    assert_eq!(document["commands"].as_object().unwrap().len(), 11);
    assert!(document["config_sources"].is_array());
}

#[test]
fn json_usage_errors_are_structured() {
    let output = run_in(&root(), &["--output", "json", "not-a-command"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let document = parse_json(&output.stderr);
    assert_eq!(document["error"]["code"], "usage");
    assert_eq!(document["error"]["exit_code"], 2);
}

#[test]
fn help_remains_successful_when_json_output_is_present() {
    let output = run_in(&root(), &["--output", "json", "--help"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: buona"));
}

#[test]
fn non_interactive_mode_refuses_setup_prompt() {
    let output = run_in(
        &root(),
        &["config", "setup", "--output", "json", "--non-interactive"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let document = parse_json(&output.stderr);
    assert!(
        document["error"]["message"]
            .as_str()
            .unwrap()
            .contains("requires terminal input")
    );
}

#[test]
fn no_color_removes_ansi_sequences() {
    let fixture = root().join("tests/fixtures/systems/cargo");
    let output = run_in(&fixture, &["detect", "--no-color"]);
    assert!(output.status.success());
    assert!(!output.stdout.windows(2).any(|window| window == b"\x1b["));
}

#[test]
fn ambiguous_hooks_have_a_structured_code_and_exit_status() {
    let fixture = root().join("tests/fixtures/invalid/ambiguous-hooks");
    let output = run_in(&fixture, &["run", "build", "--dry-run", "--output", "json"]);
    assert_eq!(output.status.code(), Some(69));
    assert!(output.stdout.is_empty());
    let document = parse_json(&output.stderr);
    assert_eq!(document["error"]["code"], "ambiguous-hook");
}

#[test]
fn malformed_target_config_is_a_structured_configuration_error() {
    let fixture = root().join("tests/fixtures/invalid/malformed");
    let output = run_in(&fixture, &["inspect", "--output", "json"]);
    assert_eq!(output.status.code(), Some(68));
    assert!(output.stdout.is_empty());
    let document = parse_json(&output.stderr);
    assert_eq!(document["error"]["code"], "configuration");
    assert!(
        document["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid buona.json")
    );
}

#[test]
fn workspace_list_json_is_sorted_and_contains_paths() {
    let temp = TempDir::new().unwrap();
    let workspaces = temp.path().join("workspaces");
    for name in ["zeta", "alpha"] {
        let directory = workspaces.join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("buona.workspace.json"),
            format!(r#"{{"name":"{name}"}}"#),
        )
        .unwrap();
    }
    write_isolated_config(
        temp.path(),
        &serde_json::json!({ "workspace_dir": workspaces }),
    );

    let output = run_with_home(
        temp.path(),
        temp.path(),
        &["workspace", "list", "--output", "json"],
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let document = parse_json(&output.stdout);
    assert_eq!(document["workspaces"][0]["name"], "alpha");
    assert_eq!(document["workspaces"][1]["name"], "zeta");
    assert!(document["workspaces"][0]["path"].is_string());
}

#[test]
fn config_mutation_json_emits_one_success_document() {
    let temp = TempDir::new().unwrap();
    write_isolated_config(
        temp.path(),
        &serde_json::json!({ "workspace_dir": temp.path().join("workspaces") }),
    );

    let output = run_with_home(
        temp.path(),
        temp.path(),
        &["config", "set", "ide", "cursor", "--output", "json"],
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let document = parse_json(&output.stdout);
    assert_eq!(document["ok"], true);
    assert_eq!(document["operation"], "config.set");
    assert_eq!(document["data"]["key"], "ide");
}
