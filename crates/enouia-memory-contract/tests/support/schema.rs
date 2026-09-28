//! In-repository JSON Schema 2020-12 *subset* validator used only by tests.
//! It is not an independent implementation of the specification. Any keyword
//! outside the supported set panics, so schemas cannot silently depend on
//! behavior this harness does not check. `format` is an annotation (as in the
//! 2020-12 default vocabulary) and is never asserted.

use super::regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$defs",
    "$comment",
    "title",
    "description",
    "format",
    "default",
    "examples",
];
const ASSERTIONS: &[&str] = &[
    "$ref",
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "propertyNames",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "pattern",
    "oneOf",
    "anyOf",
    "allOf",
    "not",
    "if",
    "then",
    "else",
    "minProperties",
    "maxProperties",
];

pub struct SchemaStore {
    root: PathBuf,
    docs: BTreeMap<String, Value>,
}

pub fn contracts_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts")
}

fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

impl SchemaStore {
    pub fn load() -> Self {
        let root = contracts_root();
        let mut docs = BTreeMap::new();
        for dir in ["memory", "context", "provider", "ipc", "store"] {
            for entry in std::fs::read_dir(root.join(dir)).expect("contract dir") {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "json") {
                    let name = format!("{dir}/{}", path.file_name().unwrap().to_string_lossy());
                    let text = std::fs::read_to_string(&path).unwrap();
                    let value: Value = serde_json::from_str(&text)
                        .unwrap_or_else(|e| panic!("{name} is not JSON: {e}"));
                    docs.insert(name, value);
                }
            }
        }
        Self { root, docs }
    }

    pub fn names(&self) -> Vec<String> {
        self.docs.keys().cloned().collect()
    }

    pub fn doc(&self, name: &str) -> &Value {
        self.docs
            .get(name)
            .unwrap_or_else(|| panic!("no schema {name} under {:?}", self.root))
    }

    /// Resolve `$ref` relative to `current` (a doc name like `memory/x.json`).
    pub fn resolve(&self, current: &str, reference: &str) -> (String, &Value) {
        let (file, fragment) = reference.split_once('#').unwrap_or((reference, ""));
        let doc = if file.is_empty() {
            current.to_owned()
        } else {
            let base = current.rsplit_once('/').map_or("", |(dir, _)| dir);
            normalize(&format!("{base}/{file}"))
        };
        let target = self
            .doc(&doc)
            .pointer(fragment)
            .unwrap_or_else(|| panic!("unresolved {reference} from {current}"));
        (doc, target)
    }

    pub fn validate(&self, doc: &str, instance: &Value) -> Vec<String> {
        let mut errors = Vec::new();
        self.node(doc, self.doc(doc), instance, "", &mut errors);
        errors
    }

    fn passes(&self, doc: &str, schema: &Value, instance: &Value) -> bool {
        let mut errors = Vec::new();
        self.node(doc, schema, instance, "", &mut errors);
        errors.is_empty()
    }

    fn node(&self, doc: &str, schema: &Value, inst: &Value, at: &str, errors: &mut Vec<String>) {
        let map = match schema {
            Value::Bool(true) => return,
            Value::Bool(false) => {
                errors.push(format!("{at}: false schema"));
                return;
            }
            Value::Object(map) => map,
            _ => panic!("schema at {doc} is not an object"),
        };
        for key in map.keys() {
            assert!(
                ANNOTATIONS.contains(&key.as_str()) || ASSERTIONS.contains(&key.as_str()),
                "unsupported schema keyword {key} in {doc}"
            );
        }
        macro_rules! fail {
            ($message:expr) => {
                errors.push(format!("{}: {}", at, $message))
            };
        }
        if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
            let (target_doc, target) = self.resolve(doc, reference);
            let mut nested = Vec::new();
            self.node(&target_doc, target, inst, at, &mut nested);
            errors.extend(nested);
        }
        if let Some(types) = map.get("type") {
            let allowed: Vec<&str> = match types {
                Value::String(t) => vec![t.as_str()],
                Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
                _ => panic!("bad type in {doc}"),
            };
            let ok = allowed.iter().any(|t| match *t {
                "null" => inst.is_null(),
                "boolean" => inst.is_boolean(),
                "object" => inst.is_object(),
                "array" => inst.is_array(),
                "string" => inst.is_string(),
                "number" => inst.is_number(),
                "integer" => inst.as_f64().is_some_and(|n| n.fract() == 0.0) && inst.is_number(),
                other => panic!("unknown type {other}"),
            });
            if !ok {
                fail!(format!("type {allowed:?}"));
                return;
            }
        }
        if let Some(values) = map.get("enum").and_then(Value::as_array)
            && !values.contains(inst)
        {
            fail!("enum");
        }
        if let Some(value) = map.get("const")
            && value != inst
        {
            fail!("const");
        }
        if let Some(n) = inst.as_f64() {
            let bound = |k: &str| map.get(k).and_then(Value::as_f64);
            if bound("minimum").is_some_and(|b| n < b)
                || bound("maximum").is_some_and(|b| n > b)
                || bound("exclusiveMinimum").is_some_and(|b| n <= b)
                || bound("exclusiveMaximum").is_some_and(|b| n >= b)
            {
                fail!("numeric bound");
            }
        }
        if let Some(s) = inst.as_str() {
            let len = s.chars().count() as u64;
            if map
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|m| len < m)
                || map
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .is_some_and(|m| len > m)
            {
                fail!("length");
            }
            if let Some(pattern) = map.get("pattern").and_then(Value::as_str)
                && !Regex::new(pattern).is_match(s)
            {
                fail!(format!("pattern {pattern}"));
            }
        }
        if let Some(items) = inst.as_array() {
            let len = items.len() as u64;
            if map
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|m| len < m)
                || map
                    .get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|m| len > m)
            {
                fail!("item count");
            }
            if map.get("uniqueItems") == Some(&Value::Bool(true)) {
                for (i, a) in items.iter().enumerate() {
                    if items[i + 1..].contains(a) {
                        fail!("uniqueItems");
                    }
                }
            }
            if let Some(item_schema) = map.get("items") {
                for (i, item) in items.iter().enumerate() {
                    self.node(doc, item_schema, item, &format!("{at}/{i}"), errors);
                }
            }
        }
        if let Some(object) = inst.as_object() {
            let len = object.len() as u64;
            if map
                .get("minProperties")
                .and_then(Value::as_u64)
                .is_some_and(|m| len < m)
                || map
                    .get("maxProperties")
                    .and_then(Value::as_u64)
                    .is_some_and(|m| len > m)
            {
                errors.push(format!("{at}: property count"));
            }
            if let Some(required) = map.get("required").and_then(Value::as_array) {
                for key in required.iter().filter_map(Value::as_str) {
                    if !object.contains_key(key) {
                        errors.push(format!("{at}: missing {key}"));
                    }
                }
            }
            let properties = map.get("properties").and_then(Value::as_object);
            for (key, value) in object {
                let child = format!("{at}/{key}");
                if let Some(names) = map.get("propertyNames") {
                    self.node(doc, names, &Value::String(key.clone()), &child, errors);
                }
                match properties.and_then(|p| p.get(key)) {
                    Some(property) => self.node(doc, property, value, &child, errors),
                    None => match map.get("additionalProperties") {
                        Some(Value::Bool(false)) => errors.push(format!("{child}: not allowed")),
                        Some(extra) => self.node(doc, extra, value, &child, errors),
                        None => {}
                    },
                }
            }
        }
        if let Some(all) = map.get("allOf").and_then(Value::as_array) {
            for sub in all {
                self.node(doc, sub, inst, at, errors);
            }
        }
        if let Some(any) = map.get("anyOf").and_then(Value::as_array)
            && !any.iter().any(|sub| self.passes(doc, sub, inst))
        {
            errors.push(format!("{at}: anyOf"));
        }
        if let Some(one) = map.get("oneOf").and_then(Value::as_array) {
            let matches = one.iter().filter(|sub| self.passes(doc, sub, inst)).count();
            if matches != 1 {
                // Report the closest branch's errors to make fixture failures readable.
                let mut best: Option<Vec<String>> = None;
                for sub in one {
                    let mut e = Vec::new();
                    self.node(doc, sub, inst, at, &mut e);
                    if best.as_ref().is_none_or(|b| e.len() < b.len()) {
                        best = Some(e);
                    }
                }
                errors.push(format!("{at}: oneOf matched {matches}"));
                if matches == 0 {
                    errors.extend(best.unwrap_or_default());
                }
            }
        }
        if let Some(not) = map.get("not")
            && self.passes(doc, not, inst)
        {
            errors.push(format!("{at}: not"));
        }
        if let Some(condition) = map.get("if") {
            let branch = if self.passes(doc, condition, inst) {
                "then"
            } else {
                "else"
            };
            if let Some(sub) = map.get(branch) {
                self.node(doc, sub, inst, at, errors);
            }
        }
    }

    /// Walk every `$ref` in every loaded schema and resolve it.
    pub fn check_all_refs(&self) -> usize {
        fn walk(store: &SchemaStore, doc: &str, value: &Value, count: &mut usize) {
            match value {
                Value::Object(map) => {
                    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                        store.resolve(doc, reference);
                        *count += 1;
                    }
                    for child in map.values() {
                        walk(store, doc, child, count);
                    }
                }
                Value::Array(items) => items.iter().for_each(|i| walk(store, doc, i, count)),
                _ => {}
            }
        }
        let mut count = 0;
        for (name, value) in &self.docs {
            assert_eq!(
                value["$schema"], "https://json-schema.org/draft/2020-12/schema",
                "{name}"
            );
            walk(self, name, value, &mut count);
        }
        count
    }
}
