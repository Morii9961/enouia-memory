//! ADR-MEM-219: the host tags a recall query's words with jieba and hands the
//! contract only the result, so the contract keeps depending on serde alone.
use std::collections::BTreeSet;

/// Nouns that frame a question rather than name its topic ("是什么时候",
/// "去过什么地方"); they count once like verbs. Found on the real library:
/// 时候 tagged as a noun pulled up every memory holding it.
pub const RECALL_FRAME_NOUNS: &[&str] = &[
    "时候", "时间", "时", "次", "地方", "东西", "事情", "事", "问题", "感觉", "样子", "方式",
    "情况", "部分", "日子", "人", "种", "个", "回", "下",
];

/// Chinese word segmentation and tagging (built-in dictionary, loaded once).
static JIEBA: std::sync::LazyLock<jieba_rs::Jieba> = std::sync::LazyLock::new(jieba_rs::Jieba::new);

/// The query's characters that lie inside its topic nouns (京都, 茶, 摄影集),
/// for `RecallExtras::naming`. Verbs, adverbs, time and measure words (去过,
/// 喜欢, 平时, 第一次) and `RECALL_FRAME_NOUNS` frame the question instead.
pub fn naming_chars(text: &str) -> BTreeSet<char> {
    JIEBA
        .tag(&text.to_lowercase(), true)
        .into_iter()
        .filter(|t| t.tag.starts_with('n') && !RECALL_FRAME_NOUNS.contains(&t.word))
        .flat_map(|t| t.word.chars())
        .collect()
}

/// ADR-MEM-225: the query's words of two or more characters as jieba cuts
/// them (大学, 摄影), for `RecallExtras::words`.
pub fn query_words(text: &str) -> BTreeSet<String> {
    JIEBA
        .cut(&text.to_lowercase(), true)
        .into_iter()
        .filter(|t| t.word.chars().count() >= 2)
        .map(|t| t.word.to_owned())
        .collect()
}
