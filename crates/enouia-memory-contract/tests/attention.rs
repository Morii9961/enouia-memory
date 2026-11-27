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
    // ADR-MEM-214: punctuation and question characters end a run.
    let question = recall_terms("我平时喜欢喝什么茶？");
    assert_eq!(question.runs, ["我平时喜欢喝", "茶"]);
    let kyoto = recall_terms("我去过京都吗");
    assert_eq!(kyoto.runs, ["我去过京都"]);
    assert_eq!(recall_terms("什么").words, Vec::<String>::new());
}

/// Synthetic memories with the given texts, as recall candidates.
/// A text may carry a title before " | " (ADR-MEM-214).
fn library(texts: &[&str]) -> Vec<MachineMemory> {
    let fields: Value =
        support::fixture("../catalog/admitted.json")["delta"]["requests"][0]["record_fields"]
            .clone();
    texts
        .iter()
        .enumerate()
        .map(|(n, text)| {
            let mut m = memory();
            m.memory.memory_id =
                MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000002")).unwrap();
            let (title, content) = text.split_once(" | ").unwrap_or(("", text));
            m.decision.input.content = content.into();
            if !title.is_empty() {
                let mut f: MemoryRecordFields = serde_json::from_value(fields.clone()).unwrap();
                f.title = title.into();
                m.decision.input.record_fields = Some(f);
            }
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
    // ADR-MEM-218: synonyms ChatGPT adds after the question only help.
    let expanded = recalled(&memories, "我第一次喝酒是什么时候？ 首次 饮酒 啤酒");
    assert_eq!(
        expanded.first(),
        Some(&memories[0].memory.memory_id),
        "{expanded:?}"
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

/// ADR-MEM-214, from Runtime's check of ADR-MEM-213: memories about 京都,
/// one naming it only in its title, lead memories that mention it once among
/// the question's common characters.
#[test]
fn titles_count_and_a_rare_word_outweighs_common_characters() {
    let memories = with_filler(
        &[
            "京都Day8：鸭川日落与宇治抹茶 | 满行程两万多步，鸭川日落，宇治抹茶",
            "京都Day9：雨中三千院 | 雨中去了三千院和琉璃光院，京都的雨很安静",
            "日本行Day7：第一次新干线到京都 | 坐新干线到了京都，夜间参拜",
            "我去过很多地方，也说过京都很好，但陪伴的意义不在去过哪里，我过得很好",
            "我去看过一部小说，主角去过的地方，我也想去，我过去常去",
            "向往东京长期生活，东京的冬天",
            "北京的朋友来看我",
        ],
        60,
    );
    let kyoto = recalled(&memories, "我去过京都吗");
    for (diary, m) in memories.iter().enumerate().take(3) {
        assert!(
            kyoto[..3].contains(&m.memory.memory_id),
            "diary {diary} among the first three: {kyoto:?}"
        );
    }
}

/// ADR-MEM-214: the memory holding the whole word 摄影集 leads a broader
/// memory about 摄影, even one that also hits another character.
#[test]
fn the_whole_word_leads_a_broader_memory() {
    let memories = with_filler(
        &[
            "主人喜爱摄影，摄影找小众地点，摄影多年，摄影器材，周末逛集市",
            "第一本摄影集Final Resonance | 第一本摄影集从设计到印成实物",
        ],
        60,
    );
    // ADR-MEM-215: a chance pairing must not hide the whole word's 集 while
    // the broader memory counts its own 集. Here 集 is rarer than 影, so 影集
    // counts as a word and the case passes either way; only the real library,
    // where 集 is common, shows the difference.
    let photos = recalled(&memories, "摄影集");
    assert_eq!(
        photos.first(),
        Some(&memories[1].memory.memory_id),
        "{photos:?}"
    );
}

/// ADR-MEM-216, with the real counts Runtime reported: both memories hold
/// 摄影集 once in the body and reach the two-mention cap, but only the photo
/// book's title names it; the broader memory repeats 摄影 six times. The
/// title naming the whole query word decides, not repeats of a sub-piece.
#[test]
fn a_title_naming_the_whole_word_beats_repeats_of_a_part() {
    let mut texts = vec![
        "第一本摄影集Final Resonance | 第一本摄影集从设计到印成实物",
        "主人喜爱摄影 | 主人喜爱摄影，摄影找小众地点，摄影多年，摄影器材，摄影展，想出一本摄影集",
        "旅行摄影计划",
        "手机摄影技巧",
        "周末去拍摄影展",
        "摄影课的作业",
    ];
    // As in the real library, 集 is common (合集, 收集, 集市...), so 影集 is
    // a chance pairing and the title's 影集 adds nothing by itself.
    texts.extend([
        "整理了一批照片的合集",
        "收集了很多贴纸",
        "周末逛集市",
        "集中精力写完报告",
        "看完了一集动画",
        "收集明信片",
        "歌单合集",
        "集体活动很热闹",
        "又追了两集剧",
        "集邮的爱好",
    ]);
    let memories = with_filler(&texts, 90);
    let photos = recalled(&memories, "摄影集");
    assert_eq!(
        photos.first(),
        Some(&memories[0].memory.memory_id),
        "{photos:?}"
    );
}

/// ADR-MEM-220: quantised vectors compare in integers; mismatches have no
/// similarity; bad model output is refused.
#[test]
fn semantic_vectors_quantise_and_compare_in_integers() {
    let a = SemanticVector::from_f32("bge-m3", &[3.0, 4.0]).unwrap();
    assert_eq!(a.values, [19660, 26214]);
    assert_eq!(semantic_milli(&a, &a), Some(1000));
    let b = SemanticVector::from_f32("bge-m3", &[-4.0, 3.0]).unwrap();
    assert_eq!(semantic_milli(&a, &b), Some(0));
    let other_model = SemanticVector::from_f32("other", &[3.0, 4.0]).unwrap();
    assert_eq!(semantic_milli(&a, &other_model), None);
    let other_size = SemanticVector::from_f32("bge-m3", &[1.0, 0.0, 0.0]).unwrap();
    assert_eq!(semantic_milli(&a, &other_size), None);
    for bad in [&[0.0f32, 0.0][..], &[f32::NAN, 1.0], &[]] {
        assert!(SemanticVector::from_f32("bge-m3", bad).is_none());
    }
    assert!(SemanticVector::from_f32("", &[1.0]).is_none());
    assert!(SemanticVector::from_f32("bge-m3", &vec![1.0; MAX_SEMANTIC_DIMS + 1]).is_none());
}

/// ADR-MEM-220: a memory similar in meaning is cued with no word in common;
/// one below the floor gains nothing; without vectors nothing changes.
#[test]
fn semantic_similarity_cues_and_ranks_memories_worded_differently() {
    let memories = with_filler(
        &[
            "从未饮酒到9月11日首次麒麟一番榨",
            "第一次参加比赛很紧张",
            "周末去爬山",
        ],
        30,
    );
    let p = profile();
    let q = query("第一回碰啤是哪年");
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    // Unit vectors at a chosen angle from the query: cos = x.
    let at = |x: f32| SemanticVector::from_f32("bge-m3", &[x, (1.0 - x * x).sqrt()]).unwrap();
    let query_vector = at(1.0);
    let (close, middling, far) = (at(0.8), at(0.6), at(0.2));
    let mut vectors: Vec<Option<&SemanticVector>> = vec![None; candidates.len()];
    vectors[0] = Some(&close);
    vectors[1] = Some(&middling);
    vectors[2] = Some(&far);
    let semantic = SemanticInput {
        query: &query_vector,
        candidates: &vectors,
    };
    let lexical = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
    let with =
        select_semantic_recall(&p, &q, &snapshot, &q.at, &candidates, None, semantic).unwrap();
    let ids = |t: &RecallTrace| {
        t.selected
            .iter()
            .map(|s| s.memory.memory_id.clone())
            .collect::<Vec<_>>()
    };
    // 0.8 is cued and leads; 0.6 gains score but is not cued without a word.
    assert_eq!(
        with.selected[0].memory.memory_id,
        memories[0].memory.memory_id,
        "{:?}",
        ids(&with)
    );
    assert!(with.selected[0].reasons.contains(&RecallReason::Semantic));
    assert!(
        !ids(&lexical).contains(&memories[0].memory.memory_id),
        "{:?}",
        ids(&lexical)
    );
    assert!(!ids(&with).contains(&memories[2].memory.memory_id));
    // No vectors at all: the trace is exactly the lexical one.
    let none: Vec<Option<&SemanticVector>> = vec![None; candidates.len()];
    let empty = SemanticInput {
        query: &query_vector,
        candidates: &none,
    };
    assert_eq!(
        select_semantic_recall(&p, &q, &snapshot, &q.at, &candidates, None, empty).unwrap(),
        lexical
    );
    // The vectors must follow the candidates one to one.
    let short = SemanticInput {
        query: &query_vector,
        candidates: &none[..1],
    };
    assert!(select_semantic_recall(&p, &q, &snapshot, &q.at, &candidates, None, short).is_err());
}

/// ADR-MEM-221: a distribution like Runtime's real library with bge-m3
/// (median about 385, deviation about 70): similarities that stand out from
/// it count, a fixed level does not.
#[test]
fn semantic_similarity_is_judged_against_its_own_distribution() {
    let background: Vec<i64> = (0..300).map(|i| 385 + (i * 37) % 241 - 120).collect();
    let with = |extra: &[i64]| {
        let mut all = background.clone();
        all.extend_from_slice(extra);
        SemanticStats::of(&all)
    };
    let kyoto = with(&[648, 640, 594, 584]);
    assert!(kyoto.deviation > 50 && kyoto.deviation < 90, "{kyoto:?}");
    assert!(kyoto.score(648).1 && kyoto.score(640).1, "{kyoto:?}");
    let (bonus, cued) = kyoto.score(584);
    assert!(bonus > 0 && !cued, "{kyoto:?}");
    assert_eq!(kyoto.score(480), (0, false));
    // An isolated top hit like the pets question (556, then 531) is no cue.
    let pets = with(&[556, 531]);
    assert!(!pets.score(556).1, "{pets:?}");
    // Below the guard nothing counts, however far out it stands.
    let low: Vec<i64> = (0..40).map(|i| 100 + i % 7).chain([450]).collect();
    assert_eq!(SemanticStats::of(&low).score(450), (0, false));
    // ADR-MEM-222: the real tea case: 517 stands 3.01 deviations out in a
    // narrow distribution but is below the guard; the bonus stops at 4.0.
    let narrow: Vec<i64> = (0..300).map(|i| 394 + (i * 37) % 141 - 70).collect();
    let tea = SemanticStats::of(&narrow.iter().copied().chain([517, 517]).collect::<Vec<_>>());
    assert_eq!(tea.score(517), (0, false), "{tea:?}");
    let far = kyoto.score(1000).0;
    assert_eq!(far, kyoto.score(990).0, "capped at 4.0 deviations");
    assert_eq!(
        far,
        (SEMANTIC_BONUS_CAP_Z100 - SEMANTIC_BONUS_Z100) as u64 * SEMANTIC_Z_SCALE as u64
    );
    // Too few similarities: the absolute thresholds still apply.
    let few = SemanticStats::of(&[700, 400, 380]);
    assert_eq!(
        few.score(700),
        (
            (700 - SEMANTIC_FLOOR_MILLI) as u64 * SEMANTIC_SCALE as u64,
            true
        )
    );
}

/// ADR-MEM-221: through recall, the one memory standing out from forty is
/// cued with no word in common; the rest are not.
#[test]
fn recall_cues_the_memory_that_stands_out() {
    let texts: Vec<String> = (0..40)
        .map(|n| format!("日常记录{n}号，平平无奇"))
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let memories = library(&refs);
    let p = profile();
    let q = query("第一回碰啤是哪年");
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let at = |x: f32| SemanticVector::from_f32("bge-m3", &[x, (1.0 - x * x).sqrt()]).unwrap();
    let query_vector = at(1.0);
    let vectors: Vec<SemanticVector> = (0..40)
        .map(|i| {
            if i == 7 {
                at(0.648)
            } else {
                at(0.3 + (i % 9) as f32 * 0.02)
            }
        })
        .collect();
    let refs: Vec<Option<&SemanticVector>> = vectors.iter().map(Some).collect();
    let trace = select_semantic_recall(
        &p,
        &q,
        &snapshot,
        &q.at,
        &candidates,
        None,
        SemanticInput {
            query: &query_vector,
            candidates: &refs,
        },
    )
    .unwrap();
    let ids: Vec<_> = trace
        .selected
        .iter()
        .map(|s| s.memory.memory_id.clone())
        .collect();
    assert_eq!(ids, vec![memories[7].memory.memory_id.clone()], "{trace:?}");
}
#[test]
fn a_bounded_trace_lists_the_near_misses_first() {
    // ADR-MEM-227: twelve memories name 茶, eight fit; the four that had a
    // cue but did not fit come before the twenty nothing pointed to.
    let mut texts: Vec<&'static str> = vec!["我喜欢喝茶"; 12];
    texts.extend(FILLER.iter().cycle().take(20));
    let memories = library(&texts);
    let p = profile();
    let q = query("茶");
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let snapshot = memories[0].request.snapshot.clone();
    let trace = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
    assert_eq!(trace.selected.len(), q.limit);
    let reasons: Vec<_> = trace.excluded.iter().map(|(_, r)| *r).collect();
    let near = 12 - q.limit;
    assert!(
        reasons[..near]
            .iter()
            .all(|r| *r == RecallExclusionReason::Budget),
        "{reasons:?}"
    );
    assert!(
        reasons[near..]
            .iter()
            .all(|r| *r == RecallExclusionReason::NoCue),
        "{reasons:?}"
    );
    // In rank order: ties fall to the memory ID, as in the selection.
    let ids: Vec<_> = trace.excluded[..near]
        .iter()
        .map(|(m, _)| &m.memory_id)
        .collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    assert!(trace.selected.last().unwrap().memory.memory_id < *ids[0]);
}
/// ADR-MEM-228: below the weakest top result that words and meaning both
/// support, a memory matched by words alone is left out.
#[test]
fn words_alone_below_a_supported_result_are_left_out() {
    let memories = with_filler(
        &["我去京都旅行了", "京都的朋友寄来明信片", "第一次去关西"],
        30,
    );
    let p = profile();
    let q = query("京都");
    let snapshot = memories[0].request.snapshot.clone();
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: q.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let at = |x: f32| SemanticVector::from_f32("bge-m3", &[x, (1.0 - x * x).sqrt()]).unwrap();
    let query_vector = at(1.0);
    let close = at(0.85);
    let background: Vec<SemanticVector> =
        (0..30).map(|n| at(0.2 + (n % 7) as f32 * 0.03)).collect();
    let run = |first: Option<&SemanticVector>| {
        let mut vectors: Vec<Option<&SemanticVector>> = vec![first, None, Some(&close)];
        vectors.extend(background.iter().map(Some));
        select_semantic_recall(
            &p,
            &q,
            &snapshot,
            &q.at,
            &candidates,
            None,
            SemanticInput {
                query: &query_vector,
                candidates: &vectors,
            },
        )
        .unwrap()
    };
    let id = |n: usize| memories[n].memory.memory_id.clone();
    let selected = |t: &RecallTrace| {
        t.selected
            .iter()
            .map(|s| s.memory.memory_id.clone())
            .collect::<Vec<_>>()
    };
    // Words and meaning support 0; 1 has the word only and scores below it.
    let with = run(Some(&close));
    assert!(
        with.selected[0].reasons.contains(&RecallReason::Semantic),
        "{with:?}"
    );
    assert_eq!(with.selected[0].memory.memory_id, id(0));
    assert!(selected(&with).contains(&id(2)), "meaning alone is kept");
    assert!(!selected(&with).contains(&id(1)));
    assert!(
        with.excluded
            .iter()
            .any(|(m, r)| m.memory_id == id(1) && *r == RecallExclusionReason::Unsupported)
    );
    // Meaning alone sets no floor: the word-only memory stays.
    let meaning_only = run(None);
    assert!(selected(&meaning_only).contains(&id(1)), "{meaning_only:?}");
    assert!(selected(&meaning_only).contains(&id(2)));
    // Without vectors nothing changes.
    let lexical = select_catalog_recall(&p, &q, &snapshot, &q.at, &candidates).unwrap();
    assert!(selected(&lexical).contains(&id(1)));
    assert!(
        lexical
            .excluded
            .iter()
            .all(|(_, r)| *r != RecallExclusionReason::Unsupported)
    );
}
