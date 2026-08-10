use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("could not parse {}: {error}", path.display()))
}

#[test]
fn representative_configs_match_their_schemas() {
    let cases = [
        (
            "schemas/config.schema.json",
            "tests/fixtures/config/config.json",
        ),
        (
            "schemas/buona.workspace.schema.json",
            "tests/fixtures/workspace/buona.workspace.json",
        ),
        (
            "schemas/buona.schema.json",
            "tests/fixtures/workspace/buona.json",
        ),
    ];

    for (schema_path, fixture_path) in cases {
        let schema = read_json(root().join(schema_path));
        let fixture = read_json(root().join(fixture_path));
        validate(&schema, &schema, &fixture, "$").unwrap_or_else(|error| {
            panic!("{fixture_path} does not satisfy {schema_path}: {error}")
        });
    }
}

#[test]
fn workspace_schema_rejects_unknown_keys() {
    let schema = read_json(root().join("schemas/buona.workspace.schema.json"));
    let mut fixture = read_json(root().join("tests/fixtures/workspace/buona.workspace.json"));
    fixture["typo"] = Value::Bool(true);
    let error = validate(&schema, &schema, &fixture, "$").unwrap_err();
    assert!(error.contains("unknown property `typo`"), "{error}");
}

#[test]
fn every_supported_system_has_a_detectable_fixture() {
    let cases = [
        ("cargo", "cargo"),
        ("go", "go"),
        ("npm", "npm"),
        ("pnpm", "pnpm"),
        ("yarn", "yarn"),
        ("bun", "bun"),
        ("uv", "uv"),
        ("poetry", "poetry"),
        ("make", "make"),
        ("just", "just"),
        ("gradle", "gradle"),
        ("maven", "maven"),
    ];

    for (directory, expected) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_buona"))
            .current_dir(root().join("tests/fixtures/systems").join(directory))
            .args(["--output", "json", "detect"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "detect failed for {directory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let document: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(document["targets"][0]["winner"], expected, "{directory}");
    }
}

#[test]
fn generated_top_level_help_is_current() {
    let expected = fs::read_to_string(root().join("docs/cli-reference.txt")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_buona"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

fn validate(root_schema: &Value, schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: only local schema refs are supported"))?;
        let resolved = root_schema
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved schema ref {reference}"))?;
        return validate(root_schema, resolved, value, path);
    }

    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return Err(format!("{path}: expected constant {constant}"));
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        return Err(format!("{path}: value {value} is not in enum"));
    }

    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        if branches
            .iter()
            .any(|branch| validate(root_schema, branch, value, path).is_ok())
        {
            return Ok(());
        }
        return Err(format!("{path}: value did not satisfy anyOf"));
    }

    if let Some(types) = schema.get("type") {
        let type_matches = match types {
            Value::String(kind) => matches_type(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| matches_type(kind, value)),
            _ => false,
        };
        if !type_matches {
            return Err(format!("{path}: unexpected JSON type for {value}"));
        }
    }

    if let Some(object) = value.as_object() {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}: missing required property `{key}`"));
                }
            }
        }

        for (key, child) in object {
            let child_path = format!("{path}.{key}");
            if let Some(child_schema) = properties.get(key) {
                validate(root_schema, child_schema, child, &child_path)?;
                continue;
            }

            match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    return Err(format!("{path}: unknown property `{key}`"));
                }
                Some(Value::Object(_)) => {
                    validate(
                        root_schema,
                        &schema["additionalProperties"],
                        child,
                        &child_path,
                    )?;
                }
                _ => {}
            }
        }
    }

    if let Some(array) = value.as_array()
        && let Some(item_schema) = schema.get("items")
    {
        for (index, child) in array.iter().enumerate() {
            validate(root_schema, item_schema, child, &format!("{path}[{index}]"))?;
        }
    }

    Ok(())
}

fn matches_type(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        _ => false,
    }
}
