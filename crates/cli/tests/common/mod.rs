// SPDX-License-Identifier: GPL-3.0-or-later
//
// Copyright (C) 2026 alplab and contributors. See LICENSE.

//! Helpers shared by the CLI integration tests: running the binary and a
//! small JSON Schema checker (the keywords the contract schema uses).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_signex-kicad-import"))
}

pub fn run(args: &[&str]) -> Output {
    bin().args(args).output().expect("the binary runs")
}

pub fn stdout_lines(out: &Output) -> Vec<Value> {
    String::from_utf8(out.stdout.clone())
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}")))
        .collect()
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("snx-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

pub fn schema() -> Value {
    let text =
        std::fs::read_to_string(fixtures().join("library-converter-v1.json")).expect("schema copy");
    serde_json::from_str(&text).expect("schema JSON")
}

/// Validate `value` against the schema node at `pointer` (for example
/// `#/$defs/describe`); returns one `path: problem` per violation.
pub fn validate(schema: &Value, pointer: &str, value: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let node = resolve(schema, pointer);
    check(schema, node, value, "$", &mut errors);
    errors
}

fn resolve<'a>(root: &'a Value, pointer: &str) -> &'a Value {
    let path = pointer.trim_start_matches('#');
    root.pointer(path)
        .unwrap_or_else(|| panic!("no schema node {pointer}"))
}

fn type_ok(kind: &str, v: &Value) -> bool {
    match kind {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        other => panic!("schema type {other} not supported by the test checker"),
    }
}

fn check(root: &Value, node: &Value, v: &Value, at: &str, errors: &mut Vec<String>) {
    if let Some(r) = node.get("$ref").and_then(Value::as_str) {
        return check(root, resolve(root, r), v, at, errors);
    }
    if let Some(t) = node.get("type") {
        let kinds: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => vec![],
        };
        if !kinds.iter().any(|k| type_ok(k, v)) {
            errors.push(format!("{at}: type {kinds:?} expected, got {v}"));
            return;
        }
    }
    if let Some(c) = node.get("const")
        && c != v
    {
        errors.push(format!("{at}: expected const {c}, got {v}"));
    }
    if let Some(Value::Array(options)) = node.get("enum")
        && !options.contains(v)
    {
        errors.push(format!("{at}: {v} not in {options:?}"));
    }
    if let (Some(p), Some(s)) = (node.get("pattern").and_then(Value::as_str), v.as_str())
        && !regex::Regex::new(p).expect("pattern").is_match(s)
    {
        errors.push(format!("{at}: {s:?} does not match {p}"));
    }
    if let (Some(n), Some(s)) = (node.get("minLength").and_then(Value::as_u64), v.as_str())
        && (s.chars().count() as u64) < n
    {
        errors.push(format!("{at}: shorter than {n}"));
    }
    if let (Some(n), Some(x)) = (node.get("minimum").and_then(Value::as_f64), v.as_f64())
        && x < n
    {
        errors.push(format!("{at}: {x} below {n}"));
    }
    if let Some(obj) = v.as_object() {
        for key in node
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = key.as_str().expect("required key");
            if !obj.contains_key(key) {
                errors.push(format!("{at}: missing {key}"));
            }
        }
        if let Some(props) = node.get("properties").and_then(Value::as_object) {
            for (key, sub) in props {
                if let Some(child) = obj.get(key) {
                    check(root, sub, child, &format!("{at}.{key}"), errors);
                }
            }
        }
    }
    if let Some(arr) = v.as_array() {
        if let Some(n) = node.get("minItems").and_then(Value::as_u64)
            && (arr.len() as u64) < n
        {
            errors.push(format!("{at}: fewer than {n} items"));
        }
        if let Some(items) = node.get("items") {
            for (i, child) in arr.iter().enumerate() {
                check(root, items, child, &format!("{at}[{i}]"), errors);
            }
        }
    }
    if let Some(Value::Array(branches)) = node.get("oneOf") {
        let passing = branches
            .iter()
            .filter(|b| {
                let mut e = Vec::new();
                check(root, b, v, at, &mut e);
                e.is_empty()
            })
            .count();
        if passing != 1 {
            errors.push(format!("{at}: {passing} oneOf branches match, expected 1"));
        }
    }
    if let Some(Value::Array(all)) = node.get("allOf") {
        for sub in all {
            check(root, sub, v, at, errors);
            if let Some(cond) = sub.get("if") {
                let mut e = Vec::new();
                check(root, cond, v, at, &mut e);
                // Already applied through `check(sub)`'s own if/then/else below.
                let _ = e;
            }
        }
    }
    if let Some(cond) = node.get("if") {
        let mut e = Vec::new();
        check(root, cond, v, at, &mut e);
        let branch = if e.is_empty() {
            node.get("then")
        } else {
            node.get("else")
        };
        if let Some(branch) = branch {
            check(root, branch, v, at, errors);
        }
    }
}
