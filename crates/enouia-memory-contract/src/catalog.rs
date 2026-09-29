//! Segmented catalog (ADR-MEM-39): how a commit is stored.
//!
//! The logical model is unchanged: a commit has a complete catalog of every
//! record's latest revision and every object (`CommitManifest` v1). On disk,
//! a commit is a [`StoredCommit`] that references immutable, content-addressed
//! [`CatalogSegment`]s; an unchanged segment is shared by hash between
//! commits, so a commit writes only the segments it changes.
//!
//! Record segments are per record kind. A record's key is the 32 hex digits
//! of its ID after the kind prefix (dashes removed); an object's key is its
//! SHA-256. The prefixes of one kind's segments are the leaves of the unique
//! partition where a prefix is a leaf when it holds at most the capacity and
//! its parent holds more (or it is the root). The partition is therefore a
//! pure function of the key set and the capacity.

use crate::commit::{
    CatalogEntry, CommitManifest, FormatVersion, ObjectEntry, ObjectKind, OperationKind,
    OperationReceipt,
};
use crate::common::{ActorRef, ActorType};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{CommitId, DeleteId, DeviceId, OperationId, ReviewId, VaultId};
use crate::json::{Revision, SchemaVersion};
use crate::record::{RecordKind, RecordRef};
use crate::store::StoreDocument;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Default leaf capacity of a new Vault.
pub const SEGMENT_CAPACITY: u64 = 512;
/// Accepted capacities (a stored commit records the one it was built with).
pub const CAPACITY_RANGE: std::ops::RangeInclusive<u64> = 16..=65_536;
/// Storage layout of a segmented Vault (descriptor and stored commits).
pub const LAYOUT_FORMAT_VERSION: u64 = 2;

/// `format_version: 2`; any other value is a different layout.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct LayoutFormat;

impl Serialize for LayoutFormat {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(LAYOUT_FORMAT_VERSION)
    }
}

impl<'de> Deserialize<'de> for LayoutFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            LAYOUT_FORMAT_VERSION => Ok(Self),
            _ => Err(serde::de::Error::custom("unsupported vault format_version")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Records,
    Objects,
}

/// Key of a record ID: the hex digits after `<prefix>_`, dashes removed.
pub fn record_key(kind: RecordKind, id: &str) -> Option<String> {
    let prefix = kind.id_prefix()?;
    let rest = id.strip_prefix(prefix)?.strip_prefix('_')?;
    crate::ids::check_prefixed_uuid(id, prefix).ok()?;
    Some(rest.chars().filter(|c| *c != '-').collect())
}

fn is_hex_prefix(prefix: &str, max: usize) -> bool {
    prefix.len() <= max
        && prefix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Key length of a segment kind: 32 for record IDs, 64 for object hashes.
pub const fn key_len(kind: SegmentKind) -> usize {
    match kind {
        SegmentKind::Records => 32,
        SegmentKind::Objects => 64,
    }
}

/// Is `leaves` (prefix → entry count) the canonical partition for its keys?
/// Leaves must be non-empty, pairwise not prefixes of each other, and each a
/// leaf by the capacity rule. Counts of inner prefixes are sums of leaves.
pub fn is_canonical(leaves: &BTreeMap<String, u64>, capacity: u64, max_len: usize) -> bool {
    let prefixes: Vec<&String> = leaves.keys().collect();
    for (i, a) in prefixes.iter().enumerate() {
        if !is_hex_prefix(a, max_len) || leaves[*a] == 0 {
            return false;
        }
        // Sorted order puts any extension of `a` right after it.
        if prefixes
            .get(i + 1)
            .is_some_and(|b| b.starts_with(a.as_str()))
        {
            return false;
        }
    }
    let count = |q: &str| -> u64 {
        leaves
            .range(q.to_owned()..)
            .take_while(|(p, _)| p.starts_with(q))
            .map(|(_, n)| n)
            .sum()
    };
    leaves.iter().all(|(prefix, n)| {
        let fits = *n <= capacity || prefix.len() == max_len;
        let parent_full = prefix.is_empty() || count(&prefix[..prefix.len() - 1]) > capacity;
        fits && parent_full
    })
}

/// Which leaf of a canonical partition a new key joins (or which new leaf it
/// starts): the leaf whose prefix it extends, else one level below its
/// deepest inner ancestor, else the root.
pub fn leaf_for(leaves: &BTreeSet<String>, key: &str) -> String {
    for len in (0..=key.len()).rev() {
        let q = &key[..len];
        if leaves.contains(q) {
            return q.to_owned();
        }
        let inner = leaves
            .range(q.to_owned()..)
            .next()
            .is_some_and(|p| p.starts_with(q) && p.len() > q.len());
        if inner {
            return key[..(len + 1).min(key.len())].to_owned();
        }
    }
    String::new()
}

/// Split `keys` (sorted, unique) under `prefix` into canonical leaves.
pub fn split(prefix: &str, keys: &[&str], capacity: u64) -> Vec<(String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    split_into(prefix, keys, 0, capacity, &mut out);
    out
}

fn split_into(
    prefix: &str,
    keys: &[&str],
    offset: usize,
    capacity: u64,
    out: &mut Vec<(String, std::ops::Range<usize>)>,
) {
    if keys.is_empty() {
        return;
    }
    let deepest = keys.iter().all(|k| k.len() <= prefix.len());
    if keys.len() as u64 <= capacity || deepest {
        out.push((prefix.to_owned(), offset..offset + keys.len()));
        return;
    }
    let mut start = 0;
    while start < keys.len() {
        let digit = &keys[start][prefix.len()..prefix.len() + 1];
        let child = format!("{prefix}{digit}");
        let end = start
            + keys[start..]
                .iter()
                .take_while(|k| k.starts_with(child.as_str()))
                .count();
        split_into(&child, &keys[start..end], offset + start, capacity, out);
        start = end;
    }
}

/// One record in a segment: its latest revision, the hashes of all earlier
/// revisions (index 0 is revision 1), and its group IDs across revisions
/// (imports a source cited; the session of an event).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEntry {
    pub record_id: String,
    pub revision: Revision,
    pub content_hash: Sha256Hex,
    pub prior_hashes: Vec<Sha256Hex>,
    pub group_ids: Vec<String>,
}

impl RecordEntry {
    /// Hash of one revision, if cataloged.
    pub fn hash_of(&self, revision: u64) -> Option<&Sha256Hex> {
        if revision == self.revision.get() {
            Some(&self.content_hash)
        } else {
            revision
                .checked_sub(1)
                .and_then(|i| self.prior_hashes.get(i as usize))
        }
    }
}

/// One object in a segment. Identity Markdown is stored beside the identity
/// revision that first named it, recorded in `stored_with`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectItem {
    pub object_hash: Sha256Hex,
    pub object_kind: ObjectKind,
    pub size_bytes: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub stored_with: Option<RecordRef>,
}

pub const fn object_kind_order(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Raw => 0,
        ObjectKind::Asset => 1,
        ObjectKind::SessionContent => 2,
        ObjectKind::IdentityMarkdown => 3,
    }
}

/// `vault/catalog/<sha256>.json`: one leaf of a kind's partition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSegment {
    pub schema_version: SchemaVersion,
    pub segment_kind: SegmentKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub record_kind: Option<RecordKind>,
    pub prefix: String,
    pub records: Vec<RecordEntry>,
    pub objects: Vec<ObjectItem>,
}

impl CatalogSegment {
    pub fn len(&self) -> usize {
        self.records.len() + self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn group_ok(kind: RecordKind, id: &str) -> bool {
    let prefix = match kind {
        RecordKind::Source => "imp",
        RecordKind::SessionEvent => "ses",
        _ => return false,
    };
    crate::ids::check_prefixed_uuid(id, prefix).is_ok()
}

impl StoreDocument for CatalogSegment {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let shape_ok = match self.segment_kind {
            SegmentKind::Records => {
                self.record_kind.is_some() && self.objects.is_empty() && !self.records.is_empty()
            }
            SegmentKind::Objects => {
                self.record_kind.is_none() && self.records.is_empty() && !self.objects.is_empty()
            }
        };
        if !shape_ok {
            out.push(Violation::new("segment.kind_entries", "/segment_kind"));
            return out;
        }
        if !is_hex_prefix(&self.prefix, key_len(self.segment_kind)) {
            out.push(Violation::new("segment.prefix", "/prefix"));
        }
        if let Some(kind) = self.record_kind {
            if !kind.is_stored() {
                out.push(Violation::new("segment.record_kind", "/record_kind"));
                return out;
            }
            let mut previous: Option<&str> = None;
            for (index, entry) in self.records.iter().enumerate() {
                let path = format!("/records/{index}");
                match record_key(kind, &entry.record_id) {
                    Some(key) if key.starts_with(&self.prefix) => {}
                    _ => out.push(Violation::new("segment.prefix_mismatch", path.clone())),
                }
                if previous.is_some_and(|p| p >= entry.record_id.as_str()) {
                    out.push(Violation::new("segment.order", path.clone()));
                }
                previous = Some(&entry.record_id);
                if !kind.is_revisioned() && entry.revision.get() != 1 {
                    out.push(Violation::new("segment.revision", path.clone()));
                }
                if entry.prior_hashes.len() as u64 != entry.revision.get() - 1 {
                    out.push(Violation::new("segment.prior_hashes", path.clone()));
                }
                let sorted = entry.group_ids.windows(2).all(|w| w[0] < w[1]);
                if !sorted || entry.group_ids.iter().any(|g| !group_ok(kind, g)) {
                    out.push(Violation::new("segment.group", path));
                }
            }
        }
        let mut previous: Option<(&Sha256Hex, u8)> = None;
        for (index, item) in self.objects.iter().enumerate() {
            let path = format!("/objects/{index}");
            if !item.object_hash.as_str().starts_with(&self.prefix) {
                out.push(Violation::new("segment.prefix_mismatch", path.clone()));
            }
            let key = (&item.object_hash, object_kind_order(item.object_kind));
            if previous.is_some_and(|p| p >= key) {
                out.push(Violation::new("segment.order", path.clone()));
            }
            previous = Some(key);
            let located = match (&item.stored_with, item.object_kind) {
                (Some(r), ObjectKind::IdentityMarkdown) => r.record_kind == RecordKind::Identity,
                (None, kind) => kind != ObjectKind::IdentityMarkdown,
                _ => false,
            };
            if !located {
                out.push(Violation::new("segment.object_location", path));
            }
        }
        out
    }
}

/// A segment reference in a stored commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRef {
    pub segment_kind: SegmentKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub record_kind: Option<RecordKind>,
    pub prefix: String,
    pub entry_count: u64,
    pub segment_hash: Sha256Hex,
}

/// `vault/commits/<commit_id>.json` in a format-2 Vault: the commit header of
/// `CommitManifest` v1 with segment references in place of the inline
/// catalog and object list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredCommit {
    pub schema_version: SchemaVersion,
    pub commit_id: CommitId,
    pub format_version: LayoutFormat,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_commit_id: Option<CommitId>,
    pub sequence: u64,
    pub vault_id: VaultId,
    pub writer_device_id: DeviceId,
    pub principal: ActorRef,
    pub operation_id: OperationId,
    pub operation_kind: OperationKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub idempotency_key_hash: Option<Sha256Hex>,
    pub request_payload_hash: Sha256Hex,
    pub created_at: Timestamp,
    pub segment_capacity: u64,
    pub record_segments: Vec<SegmentRef>,
    pub object_segments: Vec<SegmentRef>,
    pub review_ids: Vec<ReviewId>,
    pub tombstone_ids: Vec<DeleteId>,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub receipt: OperationReceipt,
}

impl StoreDocument for StoredCommit {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let genesis = self.operation_kind == OperationKind::Genesis;
        if genesis != self.parent_commit_id.is_none()
            || genesis != (self.sequence == 1)
            || self.sequence == 0
        {
            out.push(Violation::new("commit.genesis", "/parent_commit_id"));
        }
        if genesis == self.idempotency_key_hash.is_some() {
            out.push(Violation::new(
                "commit.idempotency_key",
                "/idempotency_key_hash",
            ));
        }
        if genesis && self.principal.actor_type != ActorType::Owner {
            out.push(Violation::new("commit.genesis_owner", "/principal"));
        }
        if self.receipt.operation_id != self.operation_id {
            out.push(Violation::new("commit.receipt_operation", "/receipt"));
        }
        if !CAPACITY_RANGE.contains(&self.segment_capacity) {
            out.push(Violation::new("stored.capacity", "/segment_capacity"));
            return out;
        }
        let mut per_kind: BTreeMap<&'static str, BTreeMap<String, u64>> = BTreeMap::new();
        let mut previous: Option<(&'static str, &str)> = None;
        for (index, r) in self.record_segments.iter().enumerate() {
            let path = format!("/record_segments/{index}");
            let Some(kind) = r
                .record_kind
                .filter(|k| k.is_stored() && r.segment_kind == SegmentKind::Records)
            else {
                out.push(Violation::new("stored.segment_kind", path));
                continue;
            };
            let key = (kind.name(), r.prefix.as_str());
            if previous.is_some_and(|p| p >= key) {
                out.push(Violation::new("stored.segment_order", path.clone()));
            }
            previous = Some(key);
            per_kind
                .entry(kind.name())
                .or_default()
                .insert(r.prefix.clone(), r.entry_count);
        }
        for leaves in per_kind.values() {
            if !is_canonical(leaves, self.segment_capacity, key_len(SegmentKind::Records)) {
                out.push(Violation::new("stored.partition", "/record_segments"));
            }
        }
        let mut leaves = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for (index, r) in self.object_segments.iter().enumerate() {
            let path = format!("/object_segments/{index}");
            if r.segment_kind != SegmentKind::Objects || r.record_kind.is_some() {
                out.push(Violation::new("stored.segment_kind", path));
                continue;
            }
            if previous.is_some_and(|p| p >= r.prefix.as_str()) {
                out.push(Violation::new("stored.segment_order", path));
            }
            previous = Some(&r.prefix);
            leaves.insert(r.prefix.clone(), r.entry_count);
        }
        if !leaves.is_empty()
            && !is_canonical(
                &leaves,
                self.segment_capacity,
                key_len(SegmentKind::Objects),
            )
        {
            out.push(Violation::new("stored.partition", "/object_segments"));
        }
        out
    }
}

impl StoredCommit {
    /// The complete `CommitManifest` v1 view of this commit. `segment` looks
    /// up a segment by hash; each must match its reference exactly.
    pub fn materialize<'a>(
        &self,
        segment: impl Fn(&Sha256Hex) -> Option<&'a CatalogSegment>,
    ) -> Result<CommitManifest, Vec<Violation>> {
        let mut out = Vec::new();
        let changed: BTreeSet<&RecordRef> = self.receipt.records.iter().collect();
        let mut catalog = Vec::new();
        for (index, r) in self.record_segments.iter().enumerate() {
            let Some(s) = segment(&r.segment_hash).filter(|s| matches(s, r)) else {
                out.push(Violation::new(
                    "stored.segment_mismatch",
                    format!("/record_segments/{index}"),
                ));
                continue;
            };
            let kind = s.record_kind.expect("record segment");
            for e in &s.records {
                let reference = RecordRef::new(kind, &e.record_id, e.revision);
                catalog.push(CatalogEntry {
                    changed: changed.contains(&reference),
                    record_kind: kind,
                    record_id: e.record_id.clone(),
                    revision: e.revision,
                    content_hash: e.content_hash.clone(),
                });
            }
        }
        catalog.sort_by(|a, b| {
            (a.record_kind.name(), &a.record_id).cmp(&(b.record_kind.name(), &b.record_id))
        });
        let mut objects = Vec::new();
        for (index, r) in self.object_segments.iter().enumerate() {
            let Some(s) = segment(&r.segment_hash).filter(|s| matches(s, r)) else {
                out.push(Violation::new(
                    "stored.segment_mismatch",
                    format!("/object_segments/{index}"),
                ));
                continue;
            };
            objects.extend(s.objects.iter().map(|o| ObjectEntry {
                object_hash: o.object_hash.clone(),
                size_bytes: o.size_bytes,
                object_kind: o.object_kind,
            }));
        }
        if !out.is_empty() {
            return Err(out);
        }
        let manifest = CommitManifest {
            schema_version: SchemaVersion,
            commit_id: self.commit_id.clone(),
            format_version: FormatVersion,
            parent_commit_id: self.parent_commit_id.clone(),
            sequence: self.sequence,
            vault_id: self.vault_id.clone(),
            writer_device_id: self.writer_device_id.clone(),
            principal: self.principal.clone(),
            operation_id: self.operation_id.clone(),
            operation_kind: self.operation_kind,
            idempotency_key_hash: self.idempotency_key_hash.clone(),
            request_payload_hash: self.request_payload_hash.clone(),
            created_at: self.created_at.clone(),
            catalog,
            objects,
            review_ids: self.review_ids.clone(),
            tombstone_ids: self.tombstone_ids.clone(),
            policy_epoch: self.policy_epoch,
            deletion_epoch: self.deletion_epoch,
            receipt: self.receipt.clone(),
        };
        let violations = manifest.validate();
        if violations.is_empty() {
            Ok(manifest)
        } else {
            Err(violations)
        }
    }
}

fn matches(segment: &CatalogSegment, reference: &SegmentRef) -> bool {
    segment.segment_kind == reference.segment_kind
        && segment.record_kind == reference.record_kind
        && segment.prefix == reference.prefix
        && segment.len() as u64 == reference.entry_count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(n: usize, width: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("{:0width$x}", i * 7919 % 65_536))
            .collect()
    }

    #[test]
    fn split_yields_the_canonical_partition() {
        let mut all = keys(3000, 32);
        all.sort();
        all.dedup();
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        let leaves = split("", &refs, 64);
        let counts: BTreeMap<String, u64> = leaves
            .iter()
            .map(|(p, r)| (p.clone(), r.len() as u64))
            .collect();
        assert!(counts.len() > 16);
        assert!(is_canonical(&counts, 64, 32));
        assert_eq!(counts.values().sum::<u64>(), all.len() as u64);
        // Every key lies in the leaf `leaf_for` names.
        let set: BTreeSet<String> = counts.keys().cloned().collect();
        for (prefix, range) in &leaves {
            for key in &all[range.clone()] {
                assert_eq!(&leaf_for(&set, key), prefix);
            }
        }
        // A merge-worthy split is not canonical; neither is an oversize leaf.
        let mut bad = counts.clone();
        let first = bad.keys().next().unwrap().clone();
        bad.insert(format!("{first}0"), 1);
        assert!(!is_canonical(&bad, 64, 32));
        assert!(!is_canonical(
            &BTreeMap::from([(String::new(), 65)]),
            64,
            32
        ));
        assert!(is_canonical(&BTreeMap::from([(String::new(), 64)]), 64, 32));
    }

    #[test]
    fn new_keys_join_an_existing_leaf_or_start_one_below_an_inner_prefix() {
        let leaves: BTreeSet<String> = ["0a", "0b", "1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(leaf_for(&leaves, "0a55"), "0a");
        assert_eq!(leaf_for(&leaves, "1fff"), "1");
        assert_eq!(leaf_for(&leaves, "0c00"), "0c");
        assert_eq!(leaf_for(&leaves, "7000"), "7");
        assert_eq!(leaf_for(&BTreeSet::new(), "7000"), "");
        let root: BTreeSet<String> = [String::new()].into();
        assert_eq!(leaf_for(&root, "7000"), "");
    }

    #[test]
    fn record_keys_drop_the_prefix_and_dashes() {
        assert_eq!(
            record_key(
                RecordKind::Source,
                "src_0123abcd-0000-4000-8000-00000000000f"
            )
            .as_deref(),
            Some("0123abcd00004000800000000000000f")
        );
        assert_eq!(
            record_key(
                RecordKind::Memory,
                "src_0123abcd-0000-4000-8000-00000000000f"
            ),
            None
        );
    }
}
