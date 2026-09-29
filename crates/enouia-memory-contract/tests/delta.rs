//! Scoped validation (ADR-MEM-39) equals whole-set validation.
//!
//! For every synthetic set and every single-rule mutation case, each stored
//! record in turn, and then each commit's changed records, is treated as the
//! delta on top of the rest of the set. The violations the delta adds over
//! the whole set must equal those it adds over the scoped set that a store
//! builds from `delta_needs`: all small-kind records plus the needed bulk
//! documents and commits.

mod support;

use enouia_memory_contract::delta::{Need, delta_needs, is_bulk, record_key, signed_difference};
use enouia_memory_contract::record::{AnyRecord, RecordKind};
use enouia_memory_contract::set::{RecordSet, validate_records};
use std::collections::BTreeSet;
use support::{apply_ops, fixture};

fn worlds() -> Vec<(String, RecordSet)> {
    let manifest = fixture("sets-manifest.json");
    let mut out = Vec::new();
    for file in manifest["valid"].as_array().unwrap() {
        let file = file.as_str().unwrap();
        out.push((
            file.to_owned(),
            RecordSet::from_value(&fixture(file)).unwrap(),
        ));
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let mut value = fixture(case["base"].as_str().unwrap());
        apply_ops(&mut value, &case["ops"]);
        let id = case["id"].as_str().unwrap().to_owned();
        out.push((id, RecordSet::from_value(&value).unwrap()));
    }
    out
}

/// Stored records of a set, in set order (request artifacts dropped).
fn stored(set: &RecordSet) -> Vec<AnyRecord> {
    let mut out: Vec<AnyRecord> = Vec::new();
    out.extend(set.sources.iter().cloned().map(AnyRecord::Source));
    out.extend(set.attachments.iter().cloned().map(AnyRecord::Attachment));
    out.extend(set.projects.iter().cloned().map(AnyRecord::Project));
    out.extend(
        set.memories
            .iter()
            .cloned()
            .map(|r| AnyRecord::Memory(Box::new(r))),
    );
    out.extend(
        set.candidates
            .iter()
            .cloned()
            .map(|r| AnyRecord::Candidate(Box::new(r))),
    );
    out.extend(set.reviews.iter().cloned().map(AnyRecord::Review));
    out.extend(set.identities.iter().cloned().map(AnyRecord::Identity));
    out.extend(set.sessions.iter().cloned().map(AnyRecord::Session));
    out.extend(
        set.session_events
            .iter()
            .cloned()
            .map(AnyRecord::SessionEvent),
    );
    out.extend(set.checkpoints.iter().cloned().map(AnyRecord::Checkpoint));
    out.extend(set.tombstones.iter().cloned().map(AnyRecord::Tombstone));
    out.extend(
        set.purge_receipts
            .iter()
            .cloned()
            .map(AnyRecord::PurgeReceipt),
    );
    out.extend(
        set.approvals
            .iter()
            .cloned()
            .map(|r| AnyRecord::Approval(Box::new(r))),
    );
    out.extend(
        set.policies
            .iter()
            .cloned()
            .map(|r| AnyRecord::Policy(Box::new(r))),
    );
    out.extend(
        set.imports
            .iter()
            .cloned()
            .map(|r| AnyRecord::Import(Box::new(r))),
    );
    out
}

fn kind_of(record: &AnyRecord) -> RecordKind {
    record_key(record).expect("stored").0
}

/// Does a scoped-set loader include this bulk record for `needs`?
fn needed(record: &AnyRecord, needs: &BTreeSet<Need>, all: &[AnyRecord]) -> bool {
    let (kind, id, revision) = record_key(record).expect("stored");
    needs.iter().any(|need| match need {
        Need::Revision(k, i, r) => *k == kind && *i == id && *r == revision,
        Need::AllRevisions(k, i) => *k == kind && *i == id,
        Need::SessionEvents(session) => {
            matches!(record, AnyRecord::SessionEvent(e) if e.session_id.as_str() == session)
        }
        Need::ImportSources(import) => {
            kind == RecordKind::Source
                && all.iter().any(|other| match other {
                    AnyRecord::Source(s) => {
                        s.source_id.as_str() == id
                            && s.import_id.as_ref().is_some_and(|i| i.as_str() == import)
                    }
                    _ => false,
                })
        }
        Need::Commit(_) => false,
    })
}

fn compare(label: &str, world: &RecordSet, base: &[AnyRecord], delta: &[AnyRecord]) {
    // Whole set: the base plus every commit, then the delta added.
    let mut full = RecordSet::default();
    for record in base {
        full.push(record.clone());
    }
    full.commits = world.commits.clone();
    let mut full_after = full.clone();
    for record in delta {
        full_after.push(record.clone());
    }
    let mut expected = signed_difference(&validate_records(&full), &validate_records(&full_after));
    // Only added violations decide a commit. A rule over bulk records that
    // reads small ones is monotone under additions (a new policy can only
    // resolve references), so repairs may differ; additions may not.
    expected.retain(|_, n| *n > 0);

    // Scoped: small kinds complete, bulk and commits only as needed.
    let mut small = RecordSet::default();
    for record in base.iter().filter(|r| !is_bulk(kind_of(r))) {
        small.push(record.clone());
    }
    let needs = delta_needs(&small, delta);
    let mut scoped = small;
    for record in base.iter().filter(|r| is_bulk(kind_of(r))) {
        if needed(record, &needs, base) {
            scoped.push(record.clone());
        }
    }
    for need in &needs {
        if let Need::Commit(id) = need {
            scoped.commits.extend(
                world
                    .commits
                    .iter()
                    .filter(|c| c.commit_id.as_str() == id)
                    .cloned(),
            );
        }
    }
    let mut scoped_after = scoped.clone();
    for record in delta {
        scoped_after.push(record.clone());
    }
    let mut actual =
        signed_difference(&validate_records(&scoped), &validate_records(&scoped_after));
    actual.retain(|_, n| *n > 0);
    assert_eq!(expected, actual, "{label}");
}

#[test]
fn scoped_validation_equals_whole_set_validation_for_every_single_record() {
    let mut checked = 0;
    for (name, world) in worlds() {
        let records = stored(&world);
        for index in 0..records.len() {
            let mut base = records.clone();
            let delta = vec![base.remove(index)];
            let (kind, id, rev) = record_key(&delta[0]).unwrap();
            compare(
                &format!("{name}: {kind:?} {id}@{rev}"),
                &world,
                &base,
                &delta,
            );
            checked += 1;
        }
    }
    assert!(checked > 1000, "{checked}");
}

#[test]
fn scoped_validation_equals_whole_set_validation_for_every_commit() {
    let mut checked = 0;
    for (name, world) in worlds() {
        let records = stored(&world);
        for commit in &world.commits {
            let changed: BTreeSet<(RecordKind, String, u64)> = commit
                .receipt
                .records
                .iter()
                .map(|r| (r.record_kind, r.record_id.clone(), r.revision.get()))
                .collect();
            let (delta, base): (Vec<AnyRecord>, Vec<AnyRecord>) = records
                .iter()
                .cloned()
                .partition(|r| changed.contains(&record_key(r).unwrap()));
            if delta.is_empty() {
                continue;
            }
            compare(
                &format!("{name}: commit {}", commit.commit_id),
                &world,
                &base,
                &delta,
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked}");
}

fn id_field(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Source => "source_id",
        RecordKind::Attachment => "attachment_id",
        RecordKind::Project => "project_id",
        RecordKind::Memory => "memory_id",
        RecordKind::Candidate => "candidate_id",
        RecordKind::Review => "review_id",
        RecordKind::Identity => "identity_id",
        RecordKind::Session => "session_id",
        RecordKind::SessionEvent => "event_id",
        RecordKind::Checkpoint => "checkpoint_id",
        RecordKind::Tombstone => "delete_id",
        RecordKind::PurgeReceipt => "receipt_id",
        RecordKind::Approval => "approval_id",
        RecordKind::Policy => "policy_id",
        RecordKind::Import => "import_id",
        other => panic!("not stored: {other:?}"),
    }
}

/// Deltas that the sets do not contain as such: each record copied under a
/// new ID (an extra source for a completed import, an extra event with a
/// taken sequence, a second review with the same nonce, ...), each
/// revisioned record's next revision, and an import revision that names
/// other bytes.
fn amplified(records: &[AnyRecord]) -> Vec<(String, AnyRecord)> {
    let mut out = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let (kind, id, _) = record_key(record).unwrap();
        let prefix = kind.id_prefix().unwrap();
        let mut copy = record.to_value();
        copy[id_field(kind)] = format!("{prefix}_ffffffff-ffff-4fff-bfff-{index:012x}").into();
        if let Ok(parsed) = enouia_memory_contract::parse_any(kind, &copy) {
            out.push((format!("copy of {kind:?} {id}"), parsed));
        }
        if kind.is_revisioned() {
            let next = records
                .iter()
                .filter_map(record_key)
                .filter(|(k, i, _)| *k == kind && *i == id)
                .map(|(_, _, r)| r)
                .max()
                .unwrap()
                + 1;
            let mut revised = record.to_value();
            revised["revision"] = next.into();
            if kind == RecordKind::Import {
                let mut other = revised.clone();
                other["input_object_hash"] = "e".repeat(64).into();
                if let Ok(parsed) = enouia_memory_contract::parse_any(kind, &other) {
                    out.push((format!("{kind:?} {id}@{next} other bytes"), parsed));
                }
            }
            if let Ok(parsed) = enouia_memory_contract::parse_any(kind, &revised) {
                out.push((format!("{kind:?} {id}@{next}"), parsed));
            }
        }
    }
    out
}

#[test]
fn scoped_validation_equals_whole_set_validation_for_added_copies_and_revisions() {
    let mut checked = 0;
    let mut rejected = 0;
    for (name, world) in worlds() {
        let records = stored(&world);
        for (label, record) in amplified(&records) {
            let delta = vec![record];
            let mut full = RecordSet::default();
            for r in records.iter().chain(&delta) {
                full.push(r.clone());
            }
            full.commits = world.commits.clone();
            if !validate_records(&full).is_empty() {
                rejected += 1;
            }
            compare(&format!("{name}: {label}"), &world, &records, &delta);
            checked += 1;
        }
    }
    assert!(checked > 1000 && rejected > 100, "{checked} {rejected}");
}
