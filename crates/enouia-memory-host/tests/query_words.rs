//! ADR-MEM-219: the host's jieba tagging of recall queries, and its effect
//! through the contract's selection.
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod support;
use enouia_memory_contract::{ids::MemoryId, next::*, time::Timestamp};
use enouia_memory_host::automation::query_words::naming_chars;
use serde_json::Value;

const NOW: i64 = 1_792_843_200_000;

#[test]
fn topic_nouns_name_and_frame_words_do_not() {
    let kyoto = naming_chars("我去过京都吗");
    assert!(kyoto.contains(&'京') && kyoto.contains(&'都'), "{kyoto:?}");
    assert!(
        !kyoto.contains(&'去') && !kyoto.contains(&'过'),
        "{kyoto:?}"
    );
    let drink = naming_chars("我第一次喝酒是什么时候？");
    assert!(
        !drink.contains(&'时') && !drink.contains(&'候'),
        "时候 frames: {drink:?}"
    );
    assert!(!drink.contains(&'喝'), "{drink:?}");
    assert!(naming_chars("我平时喜欢喝什么茶？").contains(&'茶'));
    assert!(naming_chars("摄影集").contains(&'集'));
    assert!(naming_chars("").is_empty());
}

fn memory(n: usize, text: &str) -> MachineMemory {
    let c: AutomationCommit =
        serde_json::from_value(support::fixture("../automation-vault/admitted.json")).unwrap();
    let mut m = c.data.memories[0].clone();
    m.memory.memory_id =
        MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000003")).unwrap();
    m.decision.input.content = text.into();
    m
}
const FILLER: [&str; 4] = [
    "我早上去跑步了，喜欢这种感觉",
    "我读过一本书，觉得不错",
    "下雨天我在家里看过电影，没什么事",
    "我喜欢晚上散步，想了很多",
];
fn recalled(texts: &[&str], query_text: &str) -> Vec<MemoryId> {
    let mut all: Vec<&str> = texts.to_vec();
    all.extend((0..60).map(|n| FILLER[n % FILLER.len()]));
    let memories: Vec<_> = all.iter().enumerate().map(|(n, t)| memory(n, t)).collect();
    let profile: AttentionProfile =
        serde_json::from_value::<Value>(support::fixture("../attention/profile.json"))
            .and_then(serde_json::from_value)
            .unwrap();
    let query = RecallQuery {
        text: query_text.into(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Current,
        limit: 8,
        max_bytes: 16384,
    };
    let candidates: Vec<_> = memories
        .iter()
        .map(|m| RecallCandidate {
            memory: m,
            source_time: query.at.clone(),
            rendered_bytes: 100,
        })
        .collect();
    let naming = naming_chars(query_text);
    select_recall_with(
        &profile,
        &query,
        &memories[0].request.snapshot,
        &query.at,
        &candidates,
        RecallExtras {
            naming: Some(&naming),
            ..RecallExtras::default()
        },
    )
    .unwrap()
    .selected
    .iter()
    .map(|s| s.memory.memory_id.clone())
    .collect()
}
fn id(n: usize) -> MemoryId {
    MemoryId::parse(&format!("mem_{n:08x}-0000-4000-8000-000000000003")).unwrap()
}

/// From Runtime's check of the noun weight: 时候 frames the question, so
/// memories holding it do not outrank the one about the first drink.
#[test]
fn frame_nouns_do_not_count_as_the_topic() {
    let drink = recalled(
        &[
            "从未饮酒到9月11日首次麒麟一番榨，酒花微苦",
            "小时候就想来东京，这时候终于来了",
            "重视重要内容被记住，什么时候都想留下",
            "看完少女终末旅行后意犹未尽，那时候很安静",
        ],
        "我第一次喝酒是什么时候？",
    );
    assert_eq!(drink.first(), Some(&id(0)), "{drink:?}");
}

/// Memories with 茶 (a noun) lead one sharing only the question's 喜欢.
#[test]
fn a_noun_in_the_question_outweighs_its_verbs() {
    let tea = recalled(
        &[
            "牙齿矫正期间少喝奶茶",
            "寿司口味偏好，配绿茶",
            "京都Day8，鸭川日落与宇治抹茶",
            "2026年初喜欢听Nanatsukaze，喜欢那种声音",
        ],
        "我平时喜欢喝什么茶？",
    );
    let at = |n: usize| tea.iter().position(|m| m == &id(n));
    if let Some(noise) = at(3) {
        for n in 0..3 {
            assert!(
                at(n).is_some_and(|p| p < noise),
                "{n} before the noise: {tea:?}"
            );
        }
    }
}
