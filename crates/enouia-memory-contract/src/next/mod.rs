//! Core-1 opt-in automation drafts (ADR-MEM-85). No Record implementation,
//! store adapter, authentication, clock, IO, or publication capability.
//! Version 2 here is independent of schema-1 records and Host CURRENT format.

mod admission;
mod attention;
mod continuation;
mod formation;
mod read_model;
mod rules;
mod storage;
mod storage_rules;

pub use admission::*;
pub use attention::*;
pub use continuation::*;
pub use formation::*;
pub use read_model::*;
pub use rules::{EvaluationContext, SourceView, TargetView, evaluate, validate_commit_binding};
pub use storage::*;
pub use storage_rules::{
    admission_batch_key, check_generation_binding, validate_delta, with_stored_context,
};

use crate::error::{ContractError, Violation};
use crate::json::check_safe_integers;
use serde::{Deserialize, Serialize};

pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
pub const MAX_SOURCES: usize = 128;

/// This is deliberately NOT crate::json::SchemaVersion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NextVersion;

impl Serialize for NextVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(2)
    }
}

impl<'de> Deserialize<'de> for NextVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            2 => Ok(Self),
            _ => Err(serde::de::Error::custom(
                "unsupported next document version",
            )),
        }
    }
}

/// Structural validation only. Evaluation/binding requires trusted current
/// context; successful parsing never admits or persists a memory.
pub trait NextDocument: Serialize + serde::de::DeserializeOwned {
    const MAX_BYTES: usize = MAX_DOCUMENT_BYTES;
    fn validate(&self) -> Vec<Violation>;
}

pub fn parse_document<T: NextDocument>(bytes: &[u8]) -> Result<T, ContractError> {
    if bytes.len() > T::MAX_BYTES {
        return Err(invalid("next.document_budget", ""));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ContractError::Malformed)?;
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    let found = value
        .get("schema_version")
        .and_then(serde_json::Value::as_i64);
    if found != Some(2) {
        return Err(ContractError::UnsupportedSchema { found });
    }
    let mut errors = Vec::new();
    check_safe_integers(&value, "", &mut errors);
    // Deserialize original bytes, so duplicate decoded keys are rejected.
    let document: T = serde_json::from_slice(bytes)
        .map_err(|_| ContractError::Shape("invalid next document".into()))?;
    errors.extend(document.validate());
    checked(errors)?;
    Ok(document)
}

pub(super) fn invalid(rule: &'static str, path: &str) -> ContractError {
    ContractError::Invalid(vec![Violation::new(rule, path)])
}

pub(super) fn checked(errors: Vec<Violation>) -> Result<(), ContractError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::Invalid(errors))
    }
}
