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

/// ADR-MEM-209/212: words stay words; CJK runs are kept whole.
#[test]
fn recall_terms_keep_words_and_cjk_runs() {
    let plain = recall_terms("moonlight Pottery");
    assert_eq!(plain.words, ["moonlight", "pottery"]);
    assert!(plain.runs.is_empty());
    let mixed = recall_terms("Enouia迁移");
    assert_eq!(mixed.words, ["enouia迁移", "enouia"]);
    assert_eq!(mixed.runs, ["迁移"]);
    let spaced = recall_terms("喝 茶 喝茶");
    assert_eq!(spaced.words, ["喝", "茶", "喝茶"]);
    assert_eq!(spaced.runs, ["喝", "茶", "喝茶"]);
    let question = recall_terms("我平时喜欢喝什么茶？");
    assert_eq!(
        question.runs,
        ["我平时喜欢喝什么茶"],
        "punctuation ends a run"
    );
}

/// Synthetic memories with the given texts, as recall candidates.
fn library(texts: &[&str]) -> Vec<MachineMemory> {
    texts
        .iter()
        .enumerate()
        .map(|(n, text)| {
            let mut m = memory();
            m.memory.memory_id =
                MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000002")).unwrap();
            m.decision.input.content = (*text).into();
            m
        })
        .collect()
}
/// The catalog and qualified selectors agree; the selected IDs in order.
fn recalled(memories: &[MachineMemory], text: &str) -> Vec<MemoryId> {
    let p = profile();
    let q = query(text);
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let states = vec![AttentionState::Active; candidates.len()];
    let catalog = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
    let qualified =
        select_qualified_catalog_recall(&p, &q, &snapshot, &q.at, &candidates, &states).unwrap();
    assert_eq!(catalog.selected, qualified.selected, "{text}");
    catalog
        .selected
        .iter()
        .map(|s| s.memory.memory_id.clone())
        .collect()
}
/// Everyday memories in Morii's voice, so 我, 去, 过, 了, 喜欢 and 什么 are
/// common, as in a real library.
const FILLER: [&str; 4] = [
    "我早上去跑步了，喜欢这种感觉",
    "我读过一本书，觉得不错",
    "下雨天我在家里看过电影，没什么事",
    "我喜欢晚上散步，想了很多",
];
fn with_filler(texts: &[&'static str], filler: usize) -> Vec<MachineMemory> {
    let mut all = texts.to_vec();
    all.extend((0..filler).map(|n| FILLER[n % FILLER.len()]));
    library(&all)
}

/// ADR-MEM-212, from Runtime's real-library check: a rare topic character
/// leads, and common question words alone do not recall.
#[test]
fn a_rare_topic_character_leads_a_chinese_question() {
    let memories = with_filler(
        &[
            "京都第八天去宇治喝了抹茶",
            "Enouia 迁移到新服务器",
            "周末去爬山",
            "第一次喝啤酒，喜欢酒花微苦",
        ],
        60,
    );
    let id = |n: usize| memories[n].memory.memory_id.clone();
    let tea = recalled(&memories, "我平时喜欢喝什么茶？");
    assert_eq!(tea.first(), Some(&id(0)), "matcha first: {tea:?}");
    assert!(!tea.contains(&id(2)), "{tea:?}");
    assert_eq!(recalled(&memories, "Enouia迁移"), vec![id(1)]);
    assert!(recalled(&memories, "滑雪").is_empty(), "no shared term");
    // Without CJK terms the old count rule applies unchanged.
    assert_eq!(recalled(&memories, "Enouia"), vec![id(1)]);
}

/// ADR-MEM-212: memories about the word asked about lead; a memory with
/// another word sharing a character (东京) or only the question's common
/// words (我去过) does not.
#[test]
fn the_word_asked_about_leads_over_shared_characters_and_question_words() {
    let memories = with_filler(
        &[
            "京都第八天，鸭川日落与宇治抹茶",
            "旅行第九天，雨中的京都三千院",
            "第一次坐新干线到京都，夜间参拜",
            "十天旅行收束：从京都到大阪",
            "向往东京长期生活，东京的冬天，东京很大",
            "我去过很多地方，自我探索应该真实",
        ],
        60,
    );
    let kyoto = recalled(&memories, "我去过京都吗");
    for (diary, m) in memories.iter().enumerate().take(4) {
        assert!(
            kyoto[..4].contains(&m.memory.memory_id),
            "diary {diary} among the first four: {kyoto:?}"
        );
    }
    assert!(
        !kyoto.contains(&memories[4].memory.memory_id),
        "东京 shares only 京: {kyoto:?}"
    );
}

/// ADR-MEM-212: the memory containing the whole word outranks one that
/// repeats part of it.
#[test]
fn the_whole_word_outranks_a_repeated_part_of_it() {
    let memories = with_filler(
        &[
            "主人喜爱摄影，摄影时会找小众地点，摄影器材很多",
            "第一本摄影集Final Resonance从设计到印成实物",
            "拥有一盏小灯，夜里拍摄用",
        ],
        60,
    );
    let photos = recalled(&memories, "摄影集");
    assert_eq!(
        photos.first(),
        Some(&memories[1].memory.memory_id),
        "{photos:?}"
    );
}

/// ADR-MEM-213, from Runtime's check of ADR-MEM-212: a memory saying 首次饮酒
/// still answers 我第一次喝酒 through the rare character 酒, and memories that
/// share only 第一次 do not outrank it.
#[test]
fn a_rare_character_finds_a_memory_worded_differently() {
    let memories = with_filler(
        &[
            "从未饮酒到9月11日首次麒麟一番榨，酒花微苦",
            "第一次参加比赛很紧张",
            "第一次去海边看日出",
        ],
        60,
    );
    let drink = recalled(&memories, "我第一次喝酒是什么时候？");
    assert_eq!(
        drink.first(),
        Some(&memories[0].memory.memory_id),
        "{drink:?}"
    );
}

/// ADR-MEM-213: among memories that share the same terms, the one returning
/// to the word asked about ranks first; repeats never outrank a stronger match.
#[test]
fn repeats_break_ties_only() {
    let memories = with_filler(
        &[
            "顺带提到京都的一句话",
            "京都第九天，京都的雨，离开京都",
            "京都第八天，宇治抹茶，京都的夜",
        ],
        60,
    );
    let kyoto = recalled(&memories, "京都");
    assert_eq!(
        kyoto.last(),
        Some(&memories[0].memory.memory_id),
        "{kyoto:?}"
    );
    assert_eq!(kyoto.len(), 3, "{kyoto:?}");
    // 抹茶 is a stronger match for this question than any number of 京都.
    let matcha = recalled(&memories, "京都抹茶");
    assert_eq!(
        matcha.first(),
        Some(&memories[2].memory.memory_id),
        "{matcha:?}"
    );
}
