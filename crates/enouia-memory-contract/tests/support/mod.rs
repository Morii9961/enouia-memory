#![allow(dead_code)]

pub mod regex;
pub mod schema;

use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/memory")
}

pub fn fixture(relative: &str) -> Value {
    let path = fixtures_root().join(relative);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').expect("pointer with parent")
}

fn container<'a>(root: &'a mut Value, pointer: &str) -> &'a mut Value {
    root.pointer_mut(pointer)
        .unwrap_or_else(|| panic!("mutation target {pointer} missing"))
}

/// Apply fixture mutation ops: `set`, `remove`, `append` (with `value` or
/// `value_from` plus optional nested `then` ops), and `repeat` (duplicate the
/// first array element until `count` items exist).
pub fn apply_ops(root: &mut Value, ops: &Value) {
    for op in ops.as_array().expect("ops array") {
        let path = op["path"].as_str().expect("path");
        match op["op"].as_str().expect("op") {
            "set" => {
                let (parent, key) = split(path);
                match container(root, parent) {
                    Value::Object(map) => {
                        map.insert(key.to_owned(), op["value"].clone());
                    }
                    Value::Array(items) => {
                        items[key.parse::<usize>().unwrap()] = op["value"].clone()
                    }
                    _ => panic!("cannot set {path}"),
                }
            }
            "remove" => {
                let (parent, key) = split(path);
                match container(root, parent) {
                    Value::Object(map) => {
                        map.remove(key).unwrap_or_else(|| panic!("remove {path}"));
                    }
                    Value::Array(items) => {
                        items.remove(key.parse::<usize>().unwrap());
                    }
                    _ => panic!("cannot remove {path}"),
                }
            }
            "append" => {
                let mut value = match op.get("value_from").and_then(Value::as_str) {
                    Some(from) => root.pointer(from).expect("value_from").clone(),
                    None => op["value"].clone(),
                };
                if let Some(then) = op.get("then") {
                    let mut wrapper = serde_json::json!({ "v": value });
                    let prefixed: Vec<Value> = then
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| {
                            let mut t = t.clone();
                            let p = format!("/v{}", t["path"].as_str().unwrap());
                            t["path"] = Value::String(p);
                            t
                        })
                        .collect();
                    apply_ops(&mut wrapper, &Value::Array(prefixed));
                    value = wrapper["v"].take();
                }
                container(root, path)
                    .as_array_mut()
                    .expect("array")
                    .push(value);
            }
            "repeat" => {
                let count = op["count"].as_u64().unwrap() as usize;
                let items = container(root, path).as_array_mut().expect("array");
                let first = items[0].clone();
                while items.len() < count {
                    items.push(first.clone());
                }
            }
            other => panic!("unknown op {other}"),
        }
    }
}
