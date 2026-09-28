//! Typed internal IDs: `<prefix>_<lowercase RFC 9562 version-4 UUID>`.
//!
//! IDs never encode time, titles, paths, or upstream account names. Upstream
//! platform IDs stay opaque strings on SourceRecord and are namespaced by
//! provider/account_scope/conversation, never reused as internal IDs.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Every ID namespace and its prefix. The JSON Schemas use the same prefixes;
/// a conformance test compares this table with `contracts/memory/common-v1`.
pub const ID_PREFIXES: &[(&str, &str)] = &[
    ("memoryId", "mem"),
    ("sourceId", "src"),
    ("attachmentId", "att"),
    ("importId", "imp"),
    ("projectId", "prj"),
    ("subjectId", "sub"),
    ("candidateId", "cand"),
    ("reviewId", "rvw"),
    ("identityId", "idn"),
    ("sessionId", "ses"),
    ("branchId", "br"),
    ("eventId", "evt"),
    ("turnId", "turn"),
    ("checkpointId", "ckp"),
    ("commitId", "cmt"),
    ("vaultId", "vlt"),
    ("deviceId", "dev"),
    ("principalId", "prn"),
    ("operationId", "op"),
    ("requestId", "req"),
    ("policyId", "pol"),
    ("deleteId", "del"),
    ("purgeReceiptId", "prg"),
    ("auditId", "aud"),
    ("capsuleId", "cap"),
    ("inspectionId", "insp"),
    ("dispatchId", "dsp"),
    ("extractionRunId", "ext"),
    ("conflictGroupId", "cfl"),
    ("itemId", "itm"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdError;

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid typed ID")
    }
}

/// Check `<prefix>_` followed by a lowercase hyphenated version-4 UUID.
pub fn check_prefixed_uuid(value: &str, prefix: &str) -> Result<(), IdError> {
    let rest = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .ok_or(IdError)?;
    if is_uuid_v4(rest) {
        Ok(())
    } else {
        Err(IdError)
    }
}

pub fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        let hyphen = matches!(index, 8 | 13 | 18 | 23);
        if hyphen != (*byte == b'-') {
            return false;
        }
        if !hyphen && !matches!(byte, b'0'..=b'9' | b'a'..=b'f') {
            return false;
        }
    }
    bytes[14] == b'4' && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
}

/// Format 16 random bytes as a version-4 UUID (version and variant bits forced).
pub fn format_uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn parse(value: &str) -> Result<Self, IdError> {
                check_prefixed_uuid(value, $prefix).map(|()| Self(value.to_owned()))
            }

            /// Build an ID from 16 bytes supplied by an `IdSource` port.
            pub fn from_random(bytes: [u8; 16]) -> Self {
                Self(format!("{}_{}", $prefix, format_uuid_v4(bytes)))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value)
                    .map_err(|_| serde::de::Error::custom(concat!("invalid ", stringify!($name))))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

typed_id!(MemoryId, "mem");
typed_id!(SourceId, "src");
typed_id!(AttachmentId, "att");
typed_id!(ImportId, "imp");
typed_id!(ProjectId, "prj");
typed_id!(SubjectId, "sub");
typed_id!(CandidateId, "cand");
typed_id!(ReviewId, "rvw");
typed_id!(IdentityId, "idn");
typed_id!(SessionId, "ses");
typed_id!(BranchId, "br");
typed_id!(EventId, "evt");
typed_id!(TurnId, "turn");
typed_id!(CheckpointId, "ckp");
typed_id!(CommitId, "cmt");
typed_id!(VaultId, "vlt");
typed_id!(DeviceId, "dev");
typed_id!(PrincipalId, "prn");
typed_id!(OperationId, "op");
typed_id!(RequestId, "req");
typed_id!(PolicyId, "pol");
typed_id!(DeleteId, "del");
typed_id!(PurgeReceiptId, "prg");
typed_id!(AuditId, "aud");
typed_id!(CapsuleId, "cap");
typed_id!(InspectionId, "insp");
typed_id!(DispatchId, "dsp");
typed_id!(ExtractionRunId, "ext");
typed_id!(ConflictGroupId, "cfl");
typed_id!(ItemId, "itm");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixed_v4_ids_are_strict() {
        assert!(MemoryId::parse("mem_00000001-0000-4000-8000-000000000001").is_ok());
        assert!(MemoryId::parse("src_00000001-0000-4000-8000-000000000001").is_err());
        assert!(MemoryId::parse("mem_00000001-0000-7000-8000-000000000001").is_err());
        assert!(MemoryId::parse("mem_00000001-0000-4000-c000-000000000001").is_err());
        assert!(MemoryId::parse("mem_00000001-0000-4000-8000-00000000000A").is_err());
        assert!(MemoryId::parse("mem_00000001000040008000000000000001").is_err());
    }

    #[test]
    fn random_bytes_become_v4_ids() {
        let id = SourceId::from_random([0xff; 16]);
        assert_eq!(id.as_str(), "src_ffffffff-ffff-4fff-bfff-ffffffffffff");
        assert!(SourceId::parse(id.as_str()).is_ok());
    }
}
