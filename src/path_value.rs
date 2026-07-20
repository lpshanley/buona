//! Shared helpers for dotted/bracket config path operations.

use anyhow::{Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathToken {
    Key(String),
    Index(usize),
}

pub(crate) fn parse_path(path: &str) -> Result<Vec<PathToken>> {
    if path.is_empty() {
        bail!("path cannot be empty");
    }

    // Structural characters ('.', '[', ']', digits) are all ASCII, so we can
    // scan bytes and slice at those positions — multi-byte UTF-8 sequences
    // never contain ASCII bytes, so every slice lands on a char boundary.
    let bytes = path.as_bytes();
    let mut i = 0usize;
    let mut tokens = Vec::new();

    while i < bytes.len() {
        if bytes[i] == b'.' {
            bail!("invalid path: unexpected '.'");
        }

        if bytes[i] == b'[' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start || i >= bytes.len() || bytes[i] != b']' {
                bail!("invalid path: malformed array index");
            }
            let idx: usize = path[start..i].parse()?;
            tokens.push(PathToken::Index(idx));
            i += 1;
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                i += 1;
            }
            let key = &path[start..i];
            if key.is_empty() {
                bail!("invalid path: empty object key");
            }
            tokens.push(PathToken::Key(key.to_string()));
        }

        if i < bytes.len() {
            if bytes[i] == b'.' {
                i += 1;
                if i >= bytes.len() {
                    bail!("invalid path: trailing '.'");
                }
            } else if bytes[i] != b'[' {
                bail!("invalid path");
            }
        }
    }

    Ok(tokens)
}

/// Walk `tokens` from `root` and return a shared reference to the target.
fn walk<'a>(root: &'a Value, tokens: &[PathToken]) -> Result<&'a Value> {
    let mut cur = root;
    for token in tokens {
        cur = match token {
            PathToken::Key(key) => cur
                .as_object()
                .ok_or_else(|| anyhow!("expected object while resolving '{key}'"))?
                .get(key)
                .ok_or_else(|| anyhow!("path not found: missing key '{key}'"))?,
            PathToken::Index(idx) => cur
                .as_array()
                .ok_or_else(|| anyhow!("expected array while resolving index"))?
                .get(*idx)
                .ok_or_else(|| anyhow!("path not found: index {idx} out of bounds"))?,
        };
    }
    Ok(cur)
}

/// Walk `tokens` from `root` and return a mutable reference to the target.
fn walk_mut<'a>(root: &'a mut Value, tokens: &[PathToken]) -> Result<&'a mut Value> {
    let mut cur = root;
    for token in tokens {
        cur = match token {
            PathToken::Key(key) => cur
                .as_object_mut()
                .ok_or_else(|| anyhow!("expected object while resolving '{key}'"))?
                .get_mut(key)
                .ok_or_else(|| anyhow!("path not found: missing key '{key}'"))?,
            PathToken::Index(idx) => cur
                .as_array_mut()
                .ok_or_else(|| anyhow!("expected array while resolving index"))?
                .get_mut(*idx)
                .ok_or_else(|| anyhow!("path not found: index {idx} out of bounds"))?,
        };
    }
    Ok(cur)
}

pub(crate) fn get_path<'a>(root: &'a Value, tokens: &[PathToken]) -> Result<&'a Value> {
    walk(root, tokens)
}

pub(crate) fn set_path(root: &mut Value, tokens: &[PathToken], value: Value) -> Result<()> {
    if tokens.is_empty() {
        *root = value;
        return Ok(());
    }

    let (parents, leaf) = tokens.split_at(tokens.len() - 1);
    let parent = walk_mut(root, parents)?;

    match &leaf[0] {
        PathToken::Key(key) => {
            parent
                .as_object_mut()
                .ok_or_else(|| anyhow!("expected object at final path segment"))?
                .insert(key.clone(), value);
        }
        PathToken::Index(idx) => {
            let arr = parent
                .as_array_mut()
                .ok_or_else(|| anyhow!("expected array at final path segment"))?;
            if *idx >= arr.len() {
                bail!("index {idx} out of bounds");
            }
            arr[*idx] = value;
        }
    }

    Ok(())
}

pub(crate) fn unset_path(root: &mut Value, tokens: &[PathToken]) -> Result<()> {
    if tokens.is_empty() {
        bail!("cannot unset root value");
    }

    let (parents, leaf) = tokens.split_at(tokens.len() - 1);
    let parent = walk_mut(root, parents)?;

    match &leaf[0] {
        PathToken::Key(key) => {
            let map = parent
                .as_object_mut()
                .ok_or_else(|| anyhow!("expected object at final path segment"))?;
            if map.remove(key).is_none() {
                bail!("path not found: missing key '{key}'");
            }
        }
        PathToken::Index(idx) => {
            let arr = parent
                .as_array_mut()
                .ok_or_else(|| anyhow!("expected array at final path segment"))?;
            if *idx >= arr.len() {
                bail!("index {idx} out of bounds");
            }
            arr.remove(*idx);
        }
    }

    Ok(())
}

/// Append values to an array at the given path, creating the array if the key
/// is missing.  Values already present are silently skipped (deduplication).
pub(crate) fn append_to_array(
    root: &mut Value,
    tokens: &[PathToken],
    values: Vec<Value>,
) -> Result<()> {
    let arr = resolve_or_create_array(root, tokens)?;
    for v in values {
        if !arr.contains(&v) {
            arr.push(v);
        }
    }
    Ok(())
}

/// Remove values from an array at the given path by value equality.
/// Silently ignores values that are not present.
pub(crate) fn remove_from_array(
    root: &mut Value,
    tokens: &[PathToken],
    values: &[Value],
) -> Result<()> {
    let arr = walk_mut(root, tokens)?
        .as_array_mut()
        .ok_or_else(|| anyhow!("expected an array at the target path"))?;
    arr.retain(|item| !values.contains(item));
    Ok(())
}

/// Resolve a path to a mutable array reference, creating an empty array if
/// the final key does not yet exist on an object.
fn resolve_or_create_array<'a>(
    root: &'a mut Value,
    tokens: &[PathToken],
) -> Result<&'a mut Vec<Value>> {
    if tokens.is_empty() {
        return root
            .as_array_mut()
            .ok_or_else(|| anyhow!("expected an array at root"));
    }

    let (parents, leaf) = tokens.split_at(tokens.len() - 1);
    let parent = walk_mut(root, parents)?;

    match &leaf[0] {
        PathToken::Key(key) => {
            let target = parent
                .as_object_mut()
                .ok_or_else(|| anyhow!("expected object at final path segment"))?
                .entry(key.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            let type_name = target_type_name(target);
            target
                .as_array_mut()
                .ok_or_else(|| anyhow!("expected an array at '{key}', found {type_name}"))
        }
        PathToken::Index(idx) => parent
            .as_array_mut()
            .ok_or_else(|| anyhow!("expected array at final path segment"))?
            .get_mut(*idx)
            .ok_or_else(|| anyhow!("index {idx} out of bounds"))?
            .as_array_mut()
            .ok_or_else(|| anyhow!("expected an array at index {idx}")),
    }
}

fn target_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Deserialize a JSON document into `T`, failing if the document contains any
/// key that `T` does not know about.
///
/// Config structs deliberately ignore unknown keys when *loading* files (so
/// legacy keys are dropped gracefully), but a `set`/`add` command writing an
/// unknown key would otherwise be silently discarded on save — a typo like
/// `buona config set workspce_dir ...` must be an error, not a no-op.
pub(crate) fn from_value_strict<T: DeserializeOwned>(value: Value) -> Result<T> {
    let mut unknown: Vec<String> = Vec::new();
    let result = serde_ignored::deserialize(value, |path| unknown.push(path.to_string()))?;
    if !unknown.is_empty() {
        bail!(
            "unknown configuration key{}: {}",
            if unknown.len() == 1 { "" } else { "s" },
            unknown.join(", ")
        );
    }
    Ok(result)
}

pub(crate) fn parse_set_value(
    raw: Option<&str>,
    json_mode: bool,
    current: Option<&Value>,
) -> Result<Value> {
    match raw {
        Some(v) => {
            if json_mode {
                Ok(serde_json::from_str(v)?)
            } else {
                Ok(parse_scalar(v))
            }
        }
        None => match current {
            Some(cur) if cur.is_boolean() => Ok(Value::Bool(true)),
            Some(_) => bail!("value is required for non-boolean fields (or pass --json)"),
            None => Ok(Value::Bool(true)),
        },
    }
}

pub(crate) fn print_value(value: &Value) -> Result<()> {
    match value {
        Value::String(s) => println!("{s}"),
        Value::Number(n) => println!("{n}"),
        Value::Bool(b) => println!("{b}"),
        Value::Null => println!("null"),
        Value::Array(_) | Value::Object(_) => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}

fn parse_scalar(raw: &str) -> Value {
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => {
            if let Ok(i) = raw.parse::<i64>() {
                return Value::Number(i.into());
            }
            if let Ok(u) = raw.parse::<u64>() {
                return Value::Number(u.into());
            }
            if let Ok(f) = raw.parse::<f64>()
                && let Some(n) = serde_json::Number::from_f64(f)
            {
                return Value::Number(n);
            }
            Value::String(raw.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_set_value_missing_target_without_value_defaults_true() {
        let v = parse_set_value(None, false, None).unwrap();
        assert_eq!(v, Value::Bool(true));
    }

    #[test]
    fn set_path_inserts_missing_object_key() {
        let mut root = serde_json::json!({"name":"ws"});
        let tokens = parse_path("mount_root").unwrap();
        set_path(&mut root, &tokens, Value::Bool(true)).unwrap();
        assert_eq!(root["mount_root"], Value::Bool(true));
    }

    #[test]
    fn parse_path_treats_kebab_as_literal_key() {
        // kebab-case is not auto-converted; user must use snake_case for config fields
        let tokens = parse_path("mount-root").unwrap();
        assert_eq!(tokens, vec![PathToken::Key("mount-root".to_string())]);
    }

    #[test]
    fn parse_path_handles_non_ascii_keys() {
        // Regression: char indices were used as byte offsets, panicking on
        // multi-byte UTF-8 keys.
        let tokens = parse_path("héllo.wörld").unwrap();
        assert_eq!(
            tokens,
            vec![
                PathToken::Key("héllo".to_string()),
                PathToken::Key("wörld".to_string()),
            ]
        );
    }

    #[test]
    fn parse_path_non_ascii_key_with_index() {
        let tokens = parse_path("héllo[3]").unwrap();
        assert_eq!(
            tokens,
            vec![PathToken::Key("héllo".to_string()), PathToken::Index(3)]
        );
    }

    #[test]
    fn append_to_array_creates_and_deduplicates() {
        let mut root = serde_json::json!({"items": ["a"]});
        let tokens = parse_path("items").unwrap();
        append_to_array(
            &mut root,
            &tokens,
            vec![Value::String("a".into()), Value::String("b".into())],
        )
        .unwrap();
        assert_eq!(root["items"], serde_json::json!(["a", "b"]));
    }

    #[test]
    fn append_to_array_creates_missing_key() {
        let mut root = serde_json::json!({"name": "test"});
        let tokens = parse_path("items").unwrap();
        append_to_array(&mut root, &tokens, vec![Value::String("x".into())]).unwrap();
        assert_eq!(root["items"], serde_json::json!(["x"]));
    }

    #[test]
    fn remove_from_array_removes_matching_values() {
        let mut root = serde_json::json!({"items": ["a", "b", "c"]});
        let tokens = parse_path("items").unwrap();
        remove_from_array(
            &mut root,
            &tokens,
            &[Value::String("b".into()), Value::String("z".into())],
        )
        .unwrap();
        assert_eq!(root["items"], serde_json::json!(["a", "c"]));
    }

    #[test]
    fn remove_from_array_noop_when_value_absent() {
        let mut root = serde_json::json!({"items": ["a"]});
        let tokens = parse_path("items").unwrap();
        remove_from_array(&mut root, &tokens, &[Value::String("z".into())]).unwrap();
        assert_eq!(root["items"], serde_json::json!(["a"]));
    }

    #[test]
    fn from_value_strict_accepts_known_fields() {
        #[derive(serde::Deserialize)]
        struct Demo {
            #[allow(dead_code)]
            name: String,
        }
        let value = serde_json::json!({"name": "ok"});
        assert!(from_value_strict::<Demo>(value).is_ok());
    }

    #[test]
    fn from_value_strict_rejects_unknown_fields() {
        #[derive(Debug, serde::Deserialize)]
        struct Demo {
            #[allow(dead_code)]
            name: String,
        }
        let value = serde_json::json!({"name": "ok", "nmae": "typo"});
        let err = from_value_strict::<Demo>(value).unwrap_err();
        assert!(err.to_string().contains("nmae"));
    }
}
