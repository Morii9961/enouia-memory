//! JSON helpers shared by every record: explicit-null fields, the schema major
//! version, revisions, extensions, and the canonical stored byte form.

use crate::error::{ContractError, Violation};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

/// The only record structure major version this build reads and writes.
pub const SCHEMA_VERSION: i64 = 1;
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Use as `#[serde(deserialize_with = "crate::json::nullable")]` on an Option
/// field: the key must be present, and `null` means explicitly unknown/absent.
/// (Serde treats a missing key as an error for fields with `deserialize_with`.)
pub fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// `schema_version: 1`. Other values fail here; callers should first use
/// [`schema_disposition`] to distinguish an unknown major from malformed input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SchemaVersion;

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match i64::deserialize(deserializer)? {
            SCHEMA_VERSION => Ok(Self),
            _ => Err(serde::de::Error::custom("unsupported schema_version")),
        }
    }
}

/// What a reader may do with a stored JSON document, decided before typed parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaDisposition {
    /// Major version 1: parse, validate, and (for writers) rewrite as new revisions.
    Supported,
    /// Another integer major: keep raw bytes, allow read-only diagnostics, never rewrite.
    UnknownMajorReadOnly(i64),
    /// Missing or non-integer version: treat as corrupt and quarantine.
    Malformed,
}

pub fn schema_disposition(value: &Value) -> SchemaDisposition {
    match value.get("schema_version").and_then(Value::as_i64) {
        Some(SCHEMA_VERSION) => SchemaDisposition::Supported,
        Some(other) => SchemaDisposition::UnknownMajorReadOnly(other),
        None => SchemaDisposition::Malformed,
    }
}

/// Only a supported major may be rewritten. Enforced before every write.
pub fn ensure_writable(value: &Value) -> Result<(), ContractError> {
    match schema_disposition(value) {
        SchemaDisposition::Supported => Ok(()),
        SchemaDisposition::UnknownMajorReadOnly(found) => {
            Err(ContractError::UnsupportedSchema { found: Some(found) })
        }
        SchemaDisposition::Malformed => Err(ContractError::UnsupportedSchema { found: None }),
    }
}

/// Positive revision of one logical record, assigned by the single writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    pub fn new(value: u64) -> Option<Self> {
        (1..=MAX_SAFE_INTEGER)
            .contains(&value)
            .then_some(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("revision out of range"))
    }
}

/// A relation that may be explicitly `"unknown"` (e.g. unrecoverable parents).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Knowable<T> {
    Known(T),
    Unknown,
}

impl<T: Serialize> Serialize for Knowable<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown => serializer.serialize_str("unknown"),
        }
    }
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Knowable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if value.as_str() == Some("unknown") {
            return Ok(Self::Unknown);
        }
        serde_json::from_value(value)
            .map(Self::Known)
            .map_err(serde::de::Error::custom)
    }
}

/// Namespaced future fields. Values are preserved verbatim on round trip.
/// Extensions never participate in access, egress, approval, or status decisions;
/// a security-relevant field requires a schema change instead. The `enouia.`
/// namespace is reserved for this project and currently defines no extension,
/// so any `enouia.*` key is rejected rather than silently ignored.
pub type Extensions = BTreeMap<String, Value>;

pub fn validate_extensions(extensions: &Extensions, path: &str, out: &mut Vec<Violation>) {
    for key in extensions.keys() {
        let well_formed = key.split('.').count() >= 2
            && key.split('.').all(|part| {
                let mut chars = part.chars();
                matches!(chars.next(), Some('a'..='z'))
                    && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
            });
        if !well_formed {
            out.push(Violation::new(
                "extensions.namespace",
                format!("{path}/{key}"),
            ));
        } else if key.starts_with("enouia.") {
            out.push(Violation::new(
                "extensions.reserved",
                format!("{path}/{key}"),
            ));
        }
    }
}

/// Canonical stored bytes: UTF-8, object keys sorted, two-space indentation,
/// LF line endings, one trailing LF. Digests are computed over these exact bytes.
pub fn canonical_bytes<T: Serialize>(record: &T) -> Result<Vec<u8>, ContractError> {
    let value = serde_json::to_value(record).map_err(|_| ContractError::Malformed)?;
    let mut bytes = serde_json::to_vec_pretty(&value).map_err(|_| ContractError::Malformed)?;
    bytes.push(b'\n');
    Ok(bytes)
}
