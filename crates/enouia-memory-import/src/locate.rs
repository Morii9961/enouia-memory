//! Resolve a source's locator against the archived bytes (I01: every
//! imported source stays reachable and its content hash verifiable).

use crate::zip::{Archive, ZipLimits};
use enouia_memory_contract::common::Locator;
use enouia_memory_contract::hash::sha256;
use serde_json::Value;

/// The content bytes a locator names, by the same rules the adapters use:
/// a JSON string's UTF-8 bytes, the `\n`-joined string `parts` of a message
/// content object, or a byte range.
pub fn resolve(raw: &[u8], locator: &Locator) -> Result<Vec<u8>, &'static str> {
    match locator {
        Locator::ByteRange { start, end } => raw
            .get(*start as usize..*end as usize)
            .map(<[u8]>::to_vec)
            .ok_or("range_out_of_bounds"),
        Locator::JsonPointer { pointer } => {
            let value: Value = serde_json::from_slice(raw).map_err(|_| "invalid_json")?;
            let target = value.pointer(pointer).ok_or("pointer_not_found")?;
            match target {
                Value::String(s) => Ok(s.as_bytes().to_vec()),
                Value::Object(obj) => Ok(obj
                    .get("parts")
                    .and_then(Value::as_array)
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
                    .into_bytes()),
                _ => Err("unsupported_target"),
            }
        }
        Locator::ArchiveMember {
            member_name,
            member_hash,
            inner,
        } => {
            let archive = Archive::open(raw, ZipLimits::default()).map_err(|e| e.code())?;
            let entry = archive.find(member_name).ok_or("member_not_found")?;
            let bytes = archive.read(entry).map_err(|p| p.code())?;
            if &sha256(&bytes) != member_hash {
                return Err("member_hash_mismatch");
            }
            resolve(&bytes, inner)
        }
        Locator::RuntimeEvent { .. } | Locator::ManualInput => Err("not_an_import_locator"),
    }
}
