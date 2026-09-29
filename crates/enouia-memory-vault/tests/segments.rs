//! Segmented catalog and scoped validation (MV-3.0, ADR-MEM-39): commits
//! rewrite only the segments they touch, the partition stays canonical,
//! the whole history still satisfies every cross-record rule, rule
//! violations that need bulk groups are still refused, and a damaged
//! segment is reported, never guessed around.

mod support;

use enouia_memory_contract::catalog::{SegmentKind, StoredCommit};
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::{Sensitivity, TimePrecision, TrustedSurface};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CommitId, PolicyId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{CommitPin, CommitRequest, IdempotencyScope, StagedRecord};
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::set::{RecordSet, validate_set};
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_contract::store::StoreDocument;
use enouia_memory_contract::{MemoryErrorCode, parse_any};
use enouia_memory_vault::service::ManualAssertionInput;
use enouia_memory_vault::{Fault, Vault};
use std::collections::BTreeSet;
use support::Harness;

const STORED: &[RecordKind] = &[
    RecordKind::Source,
    RecordKind::Attachment,
    RecordKind::Project,
    RecordKind::Memory,
    RecordKind::Candidate,
    RecordKind::Review,
    RecordKind::Identity,
    RecordKind::Session,
    RecordKind::SessionEvent,
    RecordKind::Checkpoint,
    RecordKind::Tombstone,
    RecordKind::PurgeReceipt,
    RecordKind::Approval,
    RecordKind::Policy,
    RecordKind::Import,
];

fn assertion(harness: &Harness, text: String) -> ManualAssertionInput {
    ManualAssertionInput {
        text,
        operator: harness.lifecycle.owner(),
        trusted_surface: TrustedSurface::TrustedLocalCli,
        confirmation: ConfirmationMethod::TypedConfirmation,
        sensitivity: Sensitivity::Private,
        access_policy_id: PolicyId::parse("pol_00000001-0000-4000-8000-000000000001").unwrap(),
        time_precision: TimePrecision::Millisecond,
    }
}

fn hashes(commit: &StoredCommit) -> BTreeSet<Sha256Hex> {
    commit
        .record_segments
        .iter()
        .chain(&commit.object_segments)
        .map(|r| r.segment_hash.clone())
        .collect()
}

/// Every revision of every record the pin names, and every commit of the
/// chain in its complete view.
fn whole_history(vault: &Vault, pin: &CommitPin) -> RecordSet {
    let mut set = RecordSet::default();
    for kind in STORED {
        for entry in vault.record_entries(pin, *kind).unwrap() {
            for revision in 1..=entry.revision.get() {
                let reference =
                    RecordRef::new(*kind, &entry.record_id, Revision::new(revision).unwrap());
                let bytes = vault.read_revision(pin, &reference).unwrap();
                let value = serde_json::from_slice(&bytes).unwrap();
                set.push(parse_any(*kind, &value).unwrap());
            }
        }
    }
    let mut next = Some(pin.clone());
    while let Some(at) = next {
        let manifest = vault.read_manifest(&at).unwrap();
        next = manifest.parent_commit_id.clone().map(|id| CommitPin {
            commit_id: id,
            sequence: at.sequence - 1,
            policy_epoch: 0,
            deletion_epoch: 0,
        });
        set.commits.push(manifest);
    }
    set
}

#[test]
fn commits_rewrite_only_touched_segments_and_the_history_stays_valid() {
    let harness = Harness::with_commits("segments", 18);
    let start = harness.lifecycle.time(18);
    let mut previous = harness
        .vault
        .stored_commit(&harness.vault.pin_current().unwrap())
        .unwrap();
    for n in 0..120 {
        harness.clock.set(start + 1_000 * (n + 1));
        harness
            .vault
            .record_manual_assertion(
                &assertion(&harness, format!("（合成）第 {n} 句")),
                format!("segments-{n}").as_bytes(),
            )
            .unwrap();
        let pin = harness.vault.pin_current().unwrap();
        let commit = harness.vault.stored_commit(&pin).unwrap();
        assert!(commit.validate().is_empty(), "canonical partition");
        // Only the source leaf that received the assertion was rewritten
        // (split into at most 16 children when it outgrew the capacity).
        let added: Vec<_> = commit
            .record_segments
            .iter()
            .chain(&commit.object_segments)
            .filter(|r| !hashes(&previous).contains(&r.segment_hash))
            .collect();
        assert!((1..=16).contains(&added.len()), "{n}: {}", added.len());
        assert!(
            added
                .iter()
                .all(|r| r.record_kind == Some(RecordKind::Source)),
            "{n}"
        );
        previous = commit;
    }
    let pin = harness.vault.pin_current().unwrap();
    let leaves = previous
        .record_segments
        .iter()
        .filter(|r| r.record_kind == Some(RecordKind::Source))
        .count();
    assert!(leaves > 4, "source leaves: {leaves}");
    assert!(
        previous
            .object_segments
            .iter()
            .all(|r| r.segment_kind == SegmentKind::Objects)
    );
    // The store validated each commit on a scoped set; the whole history
    // must satisfy every rule, chain and catalog closure included.
    let set = whole_history(&harness.vault, &pin);
    assert_eq!(set.commits.len() as u64, pin.sequence);
    let violations = validate_set(&set);
    assert!(violations.is_empty(), "{violations:#?}");
    assert!(harness.vault.verify(&pin).unwrap().is_clean());
}

fn owner_request(harness: &Harness, n: u32, records: Vec<StagedRecord>) -> CommitRequest {
    let owner = harness.lifecycle.owner();
    CommitRequest {
        commit_id: CommitId::parse(&format!("cmt_0000f{n:03x}-0000-4000-8000-00000000f000"))
            .unwrap(),
        expected_commit_id: None,
        idempotency: IdempotencyScope {
            principal_id: owner.actor_id.clone(),
            operation_kind: OperationKind::Import,
            key_hash: sha256(format!("scoped-{n}").as_bytes()),
        },
        principal: owner,
        operation_kind: OperationKind::Import,
        request_payload_hash: sha256(format!("scoped-{n}").as_bytes()),
        expected_revisions: Vec::new(),
        records,
        objects: Vec::new(),
    }
}

fn staged(kind: RecordKind, id: &str, value: &serde_json::Value) -> StagedRecord {
    StagedRecord {
        record_kind: kind,
        record_id: id.to_owned(),
        revision: Revision::new(value["revision"].as_u64().unwrap_or(1)).unwrap(),
        bytes: canonical_bytes(value).unwrap(),
    }
}

fn refused_with(harness: &Harness, request: CommitRequest, rule: &str) {
    let before = harness.vault.pin_current().unwrap();
    let error = harness.vault.commit(request).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::InvalidRequest, "{error}");
    match &error.fault {
        Fault::Contract(rules) => assert!(rules.contains(&rule), "{rule}: {rules:?}"),
        other => panic!("{rule}: {other:?}"),
    }
    assert_eq!(harness.vault.pin_current().unwrap(), before);
}

#[test]
fn violations_that_need_a_whole_group_are_still_refused() {
    let harness = Harness::with_commits("scoped-refusals", 18);
    harness.clock.set(harness.lifecycle.time(18) + 1_000);
    // One more source citing the completed import: its coverage no longer
    // counts exactly its sources (the store must load the import's group).
    let source_id = "src_00000067-0000-4000-8000-000000000067";
    let mut extra = harness.lifecycle.record(RecordKind::Source, source_id, 1);
    let new_id = "src_0000f001-0000-4000-8000-00000000f001";
    extra["source_id"] = new_id.into();
    refused_with(
        &harness,
        owner_request(
            &harness,
            1,
            vec![staged(RecordKind::Source, new_id, &extra)],
        ),
        "import.coverage_mismatch",
    );
    // An event that takes a sequence its session already used.
    let event_id = "evt_00000015-0000-4000-8000-000000000015";
    let mut event = harness
        .lifecycle
        .record(RecordKind::SessionEvent, event_id, 1);
    let new_event = "evt_0000f002-0000-4000-8000-00000000f002";
    event["event_id"] = new_event.into();
    refused_with(
        &harness,
        owner_request(
            &harness,
            2,
            vec![staged(RecordKind::SessionEvent, new_event, &event)],
        ),
        "event.sequence_duplicate",
    );
}

#[test]
fn a_damaged_segment_is_reported_and_reads_through_it_are_refused() {
    let harness = Harness::with_commits("segment-damage", 6);
    let pin = harness.vault.pin_current().unwrap();
    let commit = harness.vault.stored_commit(&pin).unwrap();
    let victim = commit
        .record_segments
        .iter()
        .find(|r| r.record_kind == Some(RecordKind::Source))
        .unwrap()
        .clone();
    let source = harness
        .vault
        .record_entries(&pin, RecordKind::Source)
        .unwrap()
        .into_iter()
        .find(|e| {
            enouia_memory_contract::catalog::record_key(RecordKind::Source, &e.record_id)
                .unwrap()
                .starts_with(&victim.prefix)
        })
        .unwrap();
    let path = harness
        .root
        .path()
        .join(format!("vault/catalog/{}.json", victim.segment_hash));
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 3;
    bytes[last] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();
    // A fresh handle has no cached copy. The damage is reported by range;
    // what the segment lists is refused, never guessed.
    let vault = harness.reopen(Default::default());
    let report = vault.verify(&pin).unwrap();
    assert_eq!(report.damaged_segments, vec![victim.segment_hash.clone()]);
    assert!(!report.is_clean());
    assert!(vault.read_manifest(&pin).is_err());
    let reference = RecordRef::new(RecordKind::Source, &source.record_id, source.revision);
    assert!(vault.read_record(&pin, &reference).is_err());
    // Records in other segments stay readable.
    let policy = RecordRef::new(
        RecordKind::Policy,
        "pol_00000001-0000-4000-8000-000000000001",
        Revision::new(1).unwrap(),
    );
    assert!(vault.read_record(&pin, &policy).is_ok());
}
