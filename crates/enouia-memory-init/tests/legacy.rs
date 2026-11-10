#![cfg(any(windows, target_os = "linux"))]
//! ADR-MEM-135 on synthetic data: a v1 snapshot plus the original export
//! become batches whose legacy candidates are adopted memories quoting the
//! owner's own words, inside a root the Owner bootstrap just created.
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    candidate::{CandidateRecord, CandidateStatus, ProposedType},
    common::*,
    ids::*,
    next::*,
    set::RecordSet,
    source::SourceRecord,
    time::Timestamp,
};
use enouia_memory_init::{legacy::legacy_batches, parse::*, pipeline::*};
use enouia_memory_vault::{
    OsIdSource, automation::segmented::SegmentedAutomationVault, fault::Faults,
};
use fixture::*;
use serde_json::json;
use std::sync::Arc;

fn export() -> Material {
    // Longer than a quote may be: the excerpt must pick the cited sentence.
    let filler = "（合成）今天天气不错，我们聊了很多无关的话题和琐碎的安排。".repeat(12);
    let long = format!("{filler}（合成）我喜欢安静的房间。{filler}");
    let msg = |id: &str, parent: Option<&str>, role: &str, t: i64, text: &str| {
        json!({"parent": parent, "message": {"id": id, "author": {"role": role}, "create_time": t,
            "content_lang": "zh", "content": {"parts": [text]}}})
    };
    let conversations = json!([
        {"conversation_id": "conv-a", "title": "A", "current_node": "n3", "mapping": {
            "n1": msg("a1", None, "user", 1781085600, &long),
            "n2": msg("a2", Some("n1"), "assistant", 1781085601, "（合成）明白。"),
            "n3": msg("a3", Some("n2"), "user", 1781085602, "（合成）我在做一个摄影项目。\n\t主要拍夜景。")}},
        {"conversation_id": "conv-b", "title": "B", "current_node": "n1", "mapping": {
            "n1": msg("b1", None, "user", 1781085700, "（合成）上周我去爬了山。")}},
        {"conversation_id": "conv-c", "title": "C", "current_node": "n1", "mapping": {
            "n1": msg("c1", None, "user", 1781085800, "（合成）我每周游泳。")}}
    ]);
    Material {
        alias: "legacy-export".into(),
        format: Format::Auto,
        bytes: serde_json::to_vec(&conversations).unwrap(),
        account_alias: "synthetic".into(),
    }
}

fn old_source(n: u32, conversation: &str, message: &str) -> SourceRecord {
    let mut s: SourceRecord =
        serde_json::from_value(value("memory/records/source-export-user.json")).unwrap();
    s.source_id = SourceId::parse(&id("src", 500 + n)).unwrap();
    s.original_conversation_id = Some(conversation.into());
    s.original_message_id = Some(message.into());
    s
}

fn candidate(n: u32, kind: ProposedType, text: &str, cites: &[&SourceRecord]) -> CandidateRecord {
    let mut c: CandidateRecord =
        serde_json::from_value(value("memory/records/candidate-pending.json")).unwrap();
    c.candidate_id = CandidateId::parse(&id("cand", 500 + n)).unwrap();
    c.proposed_type = kind;
    c.proposed_content = text.into();
    let template = c.evidence[0].clone();
    c.evidence = cites
        .iter()
        .map(|s| {
            let mut e = template.clone();
            e.source_id = s.source_id.clone();
            e
        })
        .collect();
    c
}

#[test]
fn legacy_candidates_become_adopted_memories_in_a_bootstrapped_root() {
    let prep = prepare(&[export()]).unwrap();
    let (a1, a3, b1, c1) = (
        old_source(1, "conv-a", "a1"),
        old_source(2, "conv-a", "a3"),
        old_source(3, "conv-b", "b1"),
        old_source(4, "conv-c", "c1"),
    );
    let gone = old_source(5, "conv-z", "z1");
    let mut rejected = candidate(4, ProposedType::Fact, "（合成）被拒绝。", &[&a1]);
    rejected.status = CandidateStatus::Rejected;
    let set = RecordSet {
        sources: vec![a1.clone(), a3.clone(), b1.clone(), c1.clone(), gone.clone()],
        candidates: vec![
            candidate(
                1,
                ProposedType::Preference,
                "（合成）喜欢安静的房间",
                &[&a1],
            ),
            // Cites two conversations: they must land in one batch.
            candidate(
                2,
                ProposedType::ProjectState,
                "（合成）摄影项目进行中",
                &[&a3, &b1],
            ),
            candidate(
                3,
                ProposedType::Episode,
                "（合成）去了不存在的地方",
                &[&gone],
            ),
            rejected,
        ],
        ..RecordSet::default()
    };
    let (batches, report) = legacy_batches(&prep, &set, 3, 128).unwrap();
    assert_eq!(report["matched_sources"], 4);
    assert_eq!(report["missing_sources"], 1);
    assert_eq!(report["statements"], 2);
    assert_eq!(report["self_reports"], 2);
    assert_eq!(report["skipped_rejected_or_deleted"], 1);
    assert_eq!(report["skipped_without_matched_evidence"], 1);
    assert_eq!(report["context_citations_kept"], 1);
    assert_eq!(report["quotes_excerpted"], 1);
    // With room for only two messages, conversation A fills a batch and the
    // citation of b1 cannot follow it: it is dropped and counted, never moved.
    let (_, tight) = legacy_batches(&prep, &set, 2, 128).unwrap();
    assert_eq!(tight["context_citations_dropped"], 1);
    assert_eq!(tight["statements"], 2);
    // Conversations A and B fill the first batch of three; C starts a second.
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].preparation.messages.len(), 3);
    assert_ne!(
        batches[0].preparation.watermark(),
        batches[1].preparation.watermark()
    );
    assert!(
        batches
            .iter()
            .flat_map(|b| &b.preparation.messages)
            .all(|m| m.role == SpeakerRole::User),
        "only messages the old library held"
    );

    // The bootstrap wrote the genesis; the initializer adopts that root.
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    drop(
        SegmentedAutomationVault::create(
            &e.root,
            e.descriptor.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap(),
    );
    let host = e.descriptor.host_id.clone();
    let mut init = Initializer::adopt_bootstrapped(
        &e.root,
        host.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    for (n, batch) in batches.iter().enumerate() {
        let plan = Plan::build(
            &batch.preparation,
            &batch.statements,
            e.policy(10 + n as u32, 1, 1),
            FormationMode::Backfill,
            Timestamp::from_unix_ms(NOW).unwrap(),
            &OsIdSource,
        )
        .unwrap();
        init.enqueue(plan, &batch.preparation).unwrap();
    }
    while init.step().unwrap() {}
    let memories = init
        .vault
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories
                .iter()
                .map(|m| {
                    let input = &m.memory.decision.input;
                    (
                        input.content.clone(),
                        m.memory.decision.outcome.adoption,
                        input.context_source_refs.len(),
                        data.sources.len(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    assert_eq!(memories.len(), 2);
    let quiet = memories
        .iter()
        .find(|m| m.0.contains("我喜欢安静的房间"))
        .unwrap();
    assert!(quiet.0.chars().count() <= 500);
    assert!(memories.iter().all(|m| m.1 == AdoptionState::Adopted));
    let photo = memories
        .iter()
        .find(|m| m.0 == "（合成）我在做一个摄影项目。\n\t主要拍夜景。")
        .unwrap();
    assert_eq!(
        photo.2, 1,
        "the second cited message is a context dependency"
    );
    assert_eq!(photo.3, 4, "every matched old source is a new source");
    assert!(
        init.plans()
            .iter()
            .all(|p| p.completed() == p.preflight.statements)
    );
    assert_eq!(init.ledger().unwrap()["extraction"]["model_invocations"], 0);
    drop(init);

    // A used root is never adopted again.
    let again =
        Initializer::adopt_bootstrapped(&e.root, host, e.clock.clone(), Arc::new(OsIdSource));
    assert!(again.is_err());
}

/// The real library kept only the owner's turns, so every turn whose assistant
/// parent is missing is its own branch. A long conversation therefore splits
/// into chunks that never share a batch, and the Vault accepts the split.
#[test]
fn a_long_user_only_conversation_splits_across_batches() {
    let mut mapping = serde_json::Map::new();
    let mut parent: Option<String> = None;
    for n in 0..80 {
        let (role, text) = if n % 2 == 0 {
            ("user", format!("（合成）第 {n} 句，我记得一件事。"))
        } else {
            ("assistant", format!("（合成）回复 {n}。"))
        };
        mapping.insert(
            format!("n{n}"),
            json!({"parent": parent, "message": {"id": format!("m{n}"), "author": {"role": role},
                "create_time": 1781085600 + n, "content_lang": "zh", "content": {"parts": [text]}}}),
        );
        parent = Some(format!("n{n}"));
    }
    let material = Material {
        alias: "legacy-long".into(),
        format: Format::Auto,
        bytes: serde_json::to_vec(&json!([{"conversation_id": "conv-long", "title": "L",
            "current_node": "n79", "mapping": mapping}]))
        .unwrap(),
        account_alias: "synthetic".into(),
    };
    let prep = prepare(&[material]).unwrap();
    let olds: Vec<_> = (0..40)
        .map(|k| old_source(k, "conv-long", &format!("m{}", 2 * k)))
        .collect();
    let set = RecordSet {
        sources: olds.clone(),
        candidates: vec![
            candidate(1, ProposedType::Fact, "（合成）早先的事", &[&olds[1]]),
            // Cites a turn in the first chunk from the second: context dropped.
            candidate(
                2,
                ProposedType::Fact,
                "（合成）后来的事",
                &[&olds[35], &olds[2]],
            ),
        ],
        ..RecordSet::default()
    };
    let (batches, report) = legacy_batches(&prep, &set, 64, 128).unwrap();
    assert_eq!(report["matched_sources"], 40);
    assert_eq!(batches.len(), 2, "32 + 8 turns, never in one batch");
    assert_eq!(report["context_citations_dropped"], 1);

    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    drop(
        SegmentedAutomationVault::create(
            &e.root,
            e.descriptor.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap(),
    );
    let mut init = Initializer::adopt_bootstrapped(
        &e.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    for (n, batch) in batches.iter().enumerate() {
        let plan = Plan::build(
            &batch.preparation,
            &batch.statements,
            e.policy(20 + n as u32, 1, 1),
            FormationMode::Backfill,
            Timestamp::from_unix_ms(NOW).unwrap(),
            &OsIdSource,
        )
        .unwrap();
        init.enqueue(plan, &batch.preparation).unwrap();
    }
    while init.step().unwrap() {}
    let (memories, sources) = init
        .vault
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            (data.memories.len(), data.sources.len())
        })
        .unwrap();
    assert_eq!((memories, sources), (2, 40));
}
