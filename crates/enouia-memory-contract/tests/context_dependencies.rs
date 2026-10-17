mod support;

use enouia_memory_contract::{
    candidate::{CandidateRecord, ProposedType},
    common::{Sensitivity, SourceRevisionRef},
    delta::{Need, delta_needs},
    ids::SourceId,
    memory::CanonicalMemory,
    record::{AnyRecord, RecordKind},
    set::{RecordSet, validate_records},
};
use support::fixture;

#[test]
fn legacy_records_keep_exact_json_without_context_claims() {
    let value = fixture("records/candidate-pending.json");
    let candidate: CandidateRecord = serde_json::from_value(value.clone()).unwrap();
    assert!(candidate.context_source_refs.is_empty());
    assert_eq!(serde_json::to_value(candidate).unwrap(), value);
    let value = fixture("records/memory-fact.json");
    let memory: CanonicalMemory = serde_json::from_value(value.clone()).unwrap();
    assert!(memory.context_source_refs.is_empty());
    assert_eq!(serde_json::to_value(memory).unwrap(), value);
}

#[test]
fn context_closure_and_floor_do_not_promote_evidence() {
    let mut world = RecordSet::from_value(&fixture("sets/lifecycle.json")).unwrap();
    let context = SourceRevisionRef {
        source_id: world.sources[1].source_id.clone(),
        source_revision: world.sources[1].revision,
    };
    assert!(
        world.memories[0]
            .evidence
            .iter()
            .all(|e| e.source_id != context.source_id)
    );
    let evidence = world.memories[0].evidence.clone();
    world.sources[1].sensitivity = Sensitivity::HighlySensitive;
    world.memories[0].context_source_refs = vec![context.clone()];
    let path = format!(
        "{}@{}",
        world.memories[0].memory_id,
        world.memories[0].revision.get()
    );
    assert!(
        validate_records(&world)
            .iter()
            .any(|v| v.rule == "sensitivity.downgrade" && v.path == path)
    );
    assert_eq!(world.memories[0].evidence, evidence);
    let record = AnyRecord::Memory(Box::new(world.memories[0].clone()));
    assert!(
        delta_needs(&RecordSet::default(), &[record]).contains(&Need::Revision(
            RecordKind::Source,
            context.source_id.to_string(),
            context.source_revision.get()
        ))
    );
    world.memories[0].context_source_refs[0].source_id = SourceId::from_random([0xe7; 16]);
    assert!(
        validate_records(&world)
            .iter()
            .any(|v| v.rule == "context_sources.unresolved" && v.path == path)
    );
    let mut candidate = world.candidates[0].clone();
    candidate.context_source_refs = vec![context];
    let record = AnyRecord::Candidate(Box::new(candidate.clone()));
    assert!(delta_needs(&RecordSet::default(), &[record]).iter().any(|need| matches!(need,
        Need::Revision(RecordKind::Source, id, _) if id == candidate.context_source_refs[0].source_id.as_str())));
}

#[test]
fn context_source_updates_load_other_dependencies() {
    let world = RecordSet::from_value(&fixture("sets/lifecycle.json")).unwrap();
    let context = SourceRevisionRef {
        source_id: world.sources[1].source_id.clone(),
        source_revision: world.sources[1].revision,
    };
    let mut memory = world.memories[0].clone();
    let mut candidate = world.candidates[0].clone();
    memory.context_source_refs = vec![context.clone()];
    candidate.context_source_refs = vec![context];
    for record in [
        AnyRecord::Memory(Box::new(memory)),
        AnyRecord::Candidate(Box::new(candidate)),
    ] {
        let mut head = RecordSet::default();
        let cited = match &record {
            AnyRecord::Memory(m) => &m.evidence[0],
            AnyRecord::Candidate(c) => &c.evidence[0],
            _ => unreachable!(),
        };
        let needed = Need::Revision(
            RecordKind::Source,
            cited.source_id.to_string(),
            cited.source_revision.get(),
        );
        assert_ne!(cited.source_id, world.sources[1].source_id);
        head.push(record);
        let delta = [AnyRecord::Source(world.sources[1].clone())];
        assert!(delta_needs(&head, &delta).contains(&needed));
    }
}

#[test]
fn capsule_requires_context_provenance_without_changing_citations() {
    let mut world = RecordSet::from_value(&fixture("sets/lifecycle.json")).unwrap();
    let item = world.capsules[0].memory_items().next().unwrap().clone();
    let source = &world.sources[1];
    assert!(
        world.capsules[0]
            .provenance
            .iter()
            .all(|p| p.source_id != source.source_id)
    );
    let memory = world
        .memories
        .iter_mut()
        .find(|m| m.memory_id == item.memory_id && m.revision == item.revision)
        .unwrap();
    memory.context_source_refs = vec![SourceRevisionRef {
        source_id: source.source_id.clone(),
        source_revision: source.revision,
    }];
    assert!(
        validate_records(&world)
            .iter()
            .any(|v| v.rule == "capsule.provenance_missing" && v.path == item.memory_id.as_str())
    );
    assert_eq!(
        world.capsules[0].memory_items().next().unwrap().evidence,
        item.evidence
    );
}

#[test]
fn context_bounds_and_identity_scope_fail_closed() {
    let mut candidate: CandidateRecord =
        serde_json::from_value(fixture("records/candidate-pending.json")).unwrap();
    let reference = SourceRevisionRef {
        source_id: candidate.source_id.clone(),
        source_revision: candidate.revision,
    };
    candidate.context_source_refs = vec![reference.clone(), reference];
    assert!(
        candidate
            .validate()
            .iter()
            .any(|v| v.rule == "context_sources.budget_or_duplicate")
    );
    candidate.context_source_refs = (0..129)
        .map(|i| SourceRevisionRef {
            source_id: SourceId::from_random([i as u8; 16]),
            source_revision: candidate.revision,
        })
        .collect();
    assert!(
        candidate
            .validate()
            .iter()
            .any(|v| v.rule == "context_sources.budget_or_duplicate")
    );
    candidate.context_source_refs.truncate(1);
    candidate.proposed_type = ProposedType::Identity;
    assert!(
        candidate
            .validate()
            .iter()
            .any(|v| v.rule == "candidate.context_scope")
    );
}
