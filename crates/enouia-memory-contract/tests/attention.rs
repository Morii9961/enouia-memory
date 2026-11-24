mod support;
#[test]
fn duplicate_native_current_observations_are_refused_instead_of_counted_twice() {
    let m = memory();
    let p = profile();
    let q = query(&m.decision.input.content);
    let candidates = [
        RecallCandidate {
            memory: &m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        },
        RecallCandidate {
            memory: &m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        },
    ];
    assert!(select_recall(&p, &q, &m.request.snapshot, &q.at, &candidates).is_err());
}
#[test]
fn scalable_recall_ranks_the_whole_authorized_catalog_and_keeps_legacy_limit() {
    let p = profile();
    let mut memories = Vec::new();
    for n in 0..180 {
        let mut m = memory();
        m.memory.memory_id =
            MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000001")).unwrap();
        m.decision.input.content = if n == 179 {
            "moonlight pottery"
        } else {
            "moonlight"
        }
        .into();
        memories.push(m);
    }
    let q = query("moonlight pottery");
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    assert!(select_recall(&p, &q, &snapshot, &q.at, &candidates).is_err());
    let large = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
    assert_eq!(large.considered, 180);
    assert_eq!(large.selected[0].memory, memories[179].memory);
    assert_eq!(large.selected.len() + large.excluded.len(), 180);
    let old = select_recall(&p, &q, &snapshot, &q.at, &candidates[..128]).unwrap();
    let compatible = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates[..128]).unwrap();
    assert_eq!(old, compatible);
}
use enouia_memory_contract::{ids::MemoryId, json::canonical_bytes, next::*, time::Timestamp};
use serde_json::Value;
use support::schema::SchemaStore;
const NOW: i64 = 1_792_843_200_000;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../attention/{name}.json"))
}
fn profile() -> AttentionProfile {
    serde_json::from_value(fixture("profile")).unwrap()
}
fn memory() -> MachineMemory {
    let c: AutomationCommit =
        serde_json::from_value(support::fixture("../automation-vault/admitted.json")).unwrap();
    c.data.memories[0].clone()
}
fn query(text: &str) -> RecallQuery {
    RecallQuery {
        text: text.into(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Current,
        limit: 8,
        max_bytes: 16384,
    }
}
fn select(p: &AttentionProfile, q: &RecallQuery, m: &MachineMemory, time: i64) -> RecallTrace {
    select_recall(
        p,
        q,
        &m.request.snapshot,
        &q.at,
        &[RecallCandidate {
            memory: m,
            source_time: Timestamp::from_unix_ms(time).unwrap(),
            rendered_bytes: 100,
        }],
    )
    .unwrap()
}
#[test]
fn profile_and_projection_schema_parity_includes_semantic_negatives() {
    let s = SchemaStore::load();
    let m = fixture("manifest");
    let parse = |name: &str, v: &Value| {
        let b = canonical_bytes(v).unwrap();
        if name == "profile.json" {
            parse_document::<AttentionProfile>(&b).is_ok()
        } else {
            parse_document::<AttentionProjection>(&b).is_ok()
        }
    };
    for e in m["valid"].as_array().unwrap() {
        let name = e["file"].as_str().unwrap();
        let v = fixture(name.trim_end_matches(".json"));
        assert!(s.validate("next/attention-v2.schema.json", &v).is_empty());
        assert!(parse(name, &v));
    }
    for c in m["invalid"].as_array().unwrap() {
        let name = c["base"].as_str().unwrap();
        let mut v = fixture(name.trim_end_matches(".json"));
        support::apply_ops(&mut v, &c["ops"]);
        assert_eq!(
            s.validate("next/attention-v2.schema.json", &v).is_empty(),
            c["schema"] == "accept",
            "{}",
            c["id"]
        );
        assert!(!parse(name, &v), "{}", c["id"]);
    }
}
#[test]
fn aliases_improve_a_fixed_lexical_gap_without_reinforcing_or_mutating_memory() {
    let mut m = memory();
    m.decision.input.content = "I enjoyed pottery and making a cup.".into();
    let before = m.clone();
    let q = query("ceramics");
    let p = profile();
    let mut baseline = p.clone();
    baseline.aliases.clear();
    assert!(select(&baseline, &q, &m, NOW).selected.is_empty());
    let trace = select(&p, &q, &m, NOW);
    assert!(trace.selected[0].reasons.contains(&RecallReason::Alias));
    assert_eq!(trace, select(&p, &q, &m, NOW));
    assert_eq!(m, before);
}
#[test]
fn fading_archives_accessibility_not_truth_and_historical_cues_rescue_eligible_material() {
    let m = memory();
    let p = profile();
    let mut q = query(&m.decision.input.content);
    let old = NOW - 200 * 86_400_000;
    assert!(select(&p, &q, &m, old).selected.is_empty());
    q.intent = RecallIntent::Historical;
    let trace = select(&p, &q, &m, old);
    assert_eq!(trace.selected[0].attention, AttentionState::Archived);
    assert!(
        trace.selected[0]
            .reasons
            .contains(&RecallReason::HistoricalRescue)
    );
    assert_eq!(m.decision.outcome.adoption, AdoptionState::Adopted);
    let mut protected = p.clone();
    protected.protected_ids.push(m.memory.memory_id.clone());
    q.intent = RecallIntent::Browse;
    q.text.clear();
    assert_eq!(
        select(&protected, &q, &m, old).selected[0].attention,
        AttentionState::Protected
    );
}
#[test]
fn condition_time_and_staging_are_hard_filters_even_for_high_relevance() {
    let mut m = memory();
    let mut dimensions: FormationDimensions =
        serde_json::from_value(support::fixture("../formation/dimensions.json")).unwrap();
    dimensions.conditions = vec!["During debugging".into()];
    m.decision.input.dimensions = Some(dimensions);
    let p = profile();
    let mut q = query(&m.decision.input.content);
    assert!(select(&p, &q, &m, NOW).selected.is_empty());
    q.conditions = vec!["During debugging".into()];
    assert_eq!(select(&p, &q, &m, NOW).selected.len(), 1);
    q.event_window = Some(FormationTime {
        from: Timestamp::from_unix_ms(NOW - 1000).unwrap(),
        until: Timestamp::from_unix_ms(NOW + 1000),
    });
    assert!(select(&p, &q, &m, NOW).selected.is_empty());
    q.event_window = None;
    m.decision.outcome.adoption = AdoptionState::Staged;
    assert_eq!(
        select(&p, &q, &m, NOW).excluded[0].1,
        RecallExclusionReason::NotAdopted
    );
}
#[test]
fn selection_counts_rendered_bytes_and_ties_replay_stably() {
    let m = memory();
    let mut other = m.clone();
    other.memory.memory_id = MemoryId::from_random([8; 16]);
    let p = profile();
    let mut q = query(&m.decision.input.content);
    q.max_bytes = 100;
    let candidates = [
        RecallCandidate {
            memory: &other,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        },
        RecallCandidate {
            memory: &m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        },
    ];
    let trace = select_recall(&p, &q, &m.request.snapshot, &q.at, &candidates).unwrap();
    assert_eq!(trace.selected.len(), 1);
    assert_eq!(trace.rendered_bytes, 100);
    assert_eq!(trace.excluded.len(), 1);
    assert_eq!(
        trace,
        select_recall(&p, &q, &m.request.snapshot, &q.at, &candidates).unwrap()
    );
}

/// ADR-MEM-209: CJK runs add two-character pieces; other words are unchanged.
#[test]
fn recall_terms_split_cjk_runs_into_pieces_and_keep_other_words() {
    assert_eq!(recall_terms("moonlight Pottery"), ["moonlight", "pottery"]);
    assert_eq!(recall_terms("摄影集"), ["摄影集", "摄影", "影集"]);
    assert_eq!(recall_terms("Enouia迁移"), ["enouia迁移", "enouia", "迁移"]);
    assert_eq!(
        recall_terms("喝 茶 喝茶"),
        ["喝", "茶", "喝茶"],
        "single characters stay words; duplicates count once"
    );
    let question = recall_terms("我平时喜欢喝什么茶？");
    for piece in ["喜欢", "欢喝", "么茶"] {
        assert!(question.contains(&piece.to_owned()), "{question:?}");
    }
    assert!(
        !question
            .iter()
            .any(|t| t.contains('？') && t.chars().count() == 2)
    );
}

/// ADR-MEM-209: a Chinese question without spaces recalls the memories that
/// share its pieces, most shared first, through every catalog entry point.
#[test]
fn chinese_questions_without_spaces_recall_by_shared_pieces() {
    let p = profile();
    let texts = [
        "我最喜欢喝乌龙茶",
        "早上喝咖啡",
        "周末去爬山",
        "Enouia 迁移到新服务器",
    ];
    let mut memories = Vec::new();
    for (n, text) in texts.iter().enumerate() {
        let mut m = memory();
        m.memory.memory_id =
            MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000002")).unwrap();
        m.decision.input.content = (*text).into();
        memories.push(m);
    }
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: query("").at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let selected = |text: &str| -> Vec<MemoryId> {
        let q = query(text);
        let states = vec![AttentionState::Active; candidates.len()];
        let catalog = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
        let qualified =
            select_qualified_catalog_recall(&p, &q, &snapshot, &q.at, &candidates, &states)
                .unwrap();
        assert_eq!(catalog.selected, qualified.selected, "{text}");
        catalog
            .selected
            .iter()
            .map(|s| s.memory.memory_id.clone())
            .collect()
    };
    let tea = selected("我平时喜欢喝什么茶？");
    assert_eq!(tea.first(), Some(&memories[0].memory.memory_id), "{tea:?}");
    assert!(!tea.contains(&memories[2].memory.memory_id), "{tea:?}");
    assert_eq!(
        selected("Enouia迁移"),
        vec![memories[3].memory.memory_id.clone()]
    );
    assert!(selected("滑雪").is_empty(), "no shared piece, no recall");
}
