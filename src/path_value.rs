//! Shared helpers for dotted/bracket config path operations.

use anyhow::{Result, bail};
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

    let chars: Vec<char> = path.chars().collect();
    let mut i = 0usize;
    let mut tokens = Vec::new();

    while i < chars.len() {
        if chars[i] == '.' {
            bail!("invalid path: unexpected '.'");
        }

        if chars[i] == '[' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i == start || i >= chars.len() || chars[i] != ']' {
                bail!("invalid path: malformed array index");
            }
            let idx: usize = path[start..i].parse()?;
            tokens.push(PathToken::Index(idx));
            i += 1;
        } else {
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                i += 1;
            }
            let key = &path[start..i];
            if key.is_empty() {
                bail!("invalid path: empty object key");
            }
            tokens.push(PathToken::Key(key.to_string()));
        }

        if i < chars.len() {
            if chars[i] == '.' {
                i += 1;
                if i >= chars.len() {
                    bail!("invalid path: trailing '.'");
                }
            } else if chars[i] != '[' {
                bail!("invalid path");
            }
        }
    }

    Ok(tokens)
}

pub(crate) fn get_path<'a>(root: &'a Value, tokens: &[PathToken]) -> Result<&'a Value> {
    let mut cur = root;
    for token in tokens {
        match token {
            PathToken::Key(key) => {
                let map = cur
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("expected object while resolving '{key}'"))?;
                cur = map
                    .get(key)
                    .ok_or_else(|| anyhow::anyhow!("path not found: missing key '{key}'"))?;
            }
            PathToken::Index(idx) => {
                let arr = cur
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("expected array while resolving index"))?;
                cur = arr
                    .get(*idx)
                    .ok_or_else(|| anyhow::anyhow!("path not found: index {idx} out of bounds"))?;
            }
        }
    }
    Ok(cur)
}

pub(crate) fn set_path(root: &mut Value, tokens: &[PathToken], value: Value) -> Result<()> {
    if tokens.is_empty() {
        *root = value;
        return Ok(());
    }

    let (parents, leaf) = tokens.split_at(tokens.len() - 1);
    let mut cur = root;

    for token in parents {
        match token {
            PathToken::Key(key) => {
                let map = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected object while resolving '{key}'"))?;
                cur = map
                    .get_mut(key)
                    .ok_or_else(|| anyhow::anyhow!("path not found: missing key '{key}'"))?;
            }
            PathToken::Index(idx) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected array while resolving index"))?;
                cur = arr
                    .get_mut(*idx)
                    .ok_or_else(|| anyhow::anyhow!("path not found: index {idx} out of bounds"))?;
            }
        }
    }

    match &leaf[0] {
        PathToken::Key(key) => {
            let map = cur
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("expected object at final path segment"))?;
            map.insert(key.clone(), value);
        }
        PathToken::Index(idx) => {
            let arr = cur
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("expected array at final path segment"))?;
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
    let mut cur = root;

    for token in parents {
        match token {
            PathToken::Key(key) => {
                let map = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected object while resolving '{key}'"))?;
                cur = map
                    .get_mut(key)
                    .ok_or_else(|| anyhow::anyhow!("path not found: missing key '{key}'"))?;
            }
            PathToken::Index(idx) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected array while resolving index"))?;
                cur = arr
                    .get_mut(*idx)
                    .ok_or_else(|| anyhow::anyhow!("path not found: index {idx} out of bounds"))?;
            }
        }
    }

    match &leaf[0] {
        PathToken::Key(key) => {
            let map = cur
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("expected object at final path segment"))?;
            if map.remove(key).is_none() {
                bail!("path not found: missing key '{key}'");
            }
        }
        PathToken::Index(idx) => {
            let arr = cur
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("expected array at final path segment"))?;
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
    // Resolve the parent so we can create or access the target array.
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
    let target = resolve_path_mut(root, tokens)?;
    let arr = target
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("expected an array at the target path"))?;
    arr.retain(|item| !values.contains(item));
    Ok(())
}

/// Walk to the target described by `tokens` and return a mutable reference.
fn resolve_path_mut<'a>(root: &'a mut Value, tokens: &[PathToken]) -> Result<&'a mut Value> {
    let mut cur = root;
    for token in tokens {
        match token {
            PathToken::Key(key) => {
                let map = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected object while resolving '{key}'"))?;
                cur = map
                    .get_mut(key)
                    .ok_or_else(|| anyhow::anyhow!("path not found: missing key '{key}'"))?;
            }
            PathToken::Index(idx) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected array while resolving index"))?;
                cur = arr
                    .get_mut(*idx)
                    .ok_or_else(|| anyhow::anyhow!("path not found: index {idx} out of bounds"))?;
            }
        }
    }
    Ok(cur)
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
            .ok_or_else(|| anyhow::anyhow!("expected an array at root"));
    }

    let (parents, leaf) = tokens.split_at(tokens.len() - 1);
    let mut cur = root;

    for token in parents {
        match token {
            PathToken::Key(key) => {
                let map = cur
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected object while resolving '{key}'"))?;
                cur = map
                    .get_mut(key)
                    .ok_or_else(|| anyhow::anyhow!("path not found: missing key '{key}'"))?;
            }
            PathToken::Index(idx) => {
                let arr = cur
                    .as_array_mut()
                    .ok_or_else(|| anyhow::anyhow!("expected array while resolving index"))?;
                cur = arr
                    .get_mut(*idx)
                    .ok_or_else(|| anyhow::anyhow!("path not found: index {idx} out of bounds"))?;
            }
        }
    }

    match &leaf[0] {
        PathToken::Key(key) => {
            let map = cur
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("expected object at final path segment"))?;
            // Create an empty array if the key doesn't exist yet.
            if !map.contains_key(key) {
                map.insert(key.clone(), Value::Array(Vec::new()));
            }
            let target = map.get_mut(key).unwrap();
            let type_name = target_type_name(target);
            target
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("expected an array at '{key}', found {type_name}"))
        }
        PathToken::Index(idx) => {
            let arr = cur
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("expected array at final path segment"))?;
            let target = arr
                .get_mut(*idx)
                .ok_or_else(|| anyhow::anyhow!("index {idx} out of bounds"))?;
            target
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("expected an array at index {idx}"))
        }
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
}
