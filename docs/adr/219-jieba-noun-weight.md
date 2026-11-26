# ADR-MEM-219 — jieba tags the query; topic nouns weigh twice

Date: 2026-11-26. Status: Accepted (Morii: "能提升性能的话你就迁移一下吧";
merged after the Runtime session's real-library check found no regression).

## Context

Morii asked whether jieba word segmentation, as used by Ombre-Brain, would
improve recall, and to adopt it if it did.

A first attempt replaced the CJK scoring of ADR-MEM-216 with jieba words and
BM25. It was worse on the cases learned from the real library:

- **Whole-word matching misses partial ones.** Matching only whole words
  lost the character matches that answer 喝酒 with 饮酒 and 茶 with 抹茶.
- **Frame words look like topics.** BM25 weighted question words such as 去过
  and 第一次 like rare topic words.

That version was dropped.

## Decision

The validated scoring of ADR-MEM-209 to 216 stays. jieba-rs 0.11 (MIT, built-in
dictionary, loaded once) only tags the recall query's words. The tagging lives
in the host (`automation::query_words::naming_chars`), because the contract
depends on serde alone (the D03/D04 boundary test). The host passes the
contract the query's topic-noun characters as data, through the new
`RecallExtras { attention, semantic, naming }` of `select_recall_with`. Both
host recall paths (recall preview and qualified Agent recall) use it.

- **Topic nouns.** A strong CJK piece or character that lies inside a noun of
  the query counts twice (京都, 茶, 摄影集).
- **Everything else.** Verbs, adverbs, time and measure words (去过, 喜欢,
  平时, 第一次) count once.
- **Frame nouns.** These are nouns that frame a question rather than name its
  topic: 时候, 时间, 地方, 东西 and others, listed in `RECALL_FRAME_NOUNS`.
  They also count once. On the real library, 时候 tagged as a noun first
  pulled up every memory containing it.

Queries without CJK are unchanged.

**Dependency.** jieba-rs, a dependency of `enouia-memory-host` only, adds a
compressed dictionary of a few MB. `zstd-sys` compiles C at build time.

## Verification

- **Host tests.** `enouia-memory-host` `query_words`:
  - `naming_chars` keeps 京都, 茶 and 摄影集, and drops 去过, 喝 and 时候.
  - `frame_nouns_do_not_count_as_the_topic` reproduces the first real-library
    regression: it fails without 时候 in the frame-noun list and passes with
    it.
  - The tea test also passes without the weight. Only the real library
    could show that gain.
- **Contract tests.** The 20 `attention` tests and the boundary tests pass.
- **Real library.** Runtime checked 0b4d1c1, read-only, against the scoring of
  b7a0269/bac2dbe, at 16 KB and 64 KB:
  - "我去过京都吗": the top two at 16 KB are Kyoto diaries, where a stray
    memory used to lead. At 64 KB the top five are all Kyoto records.
  - "我第一次喝酒是什么时候？": unchanged, and the right memory still leads.
  - "我平时喜欢喝什么茶？": the Uji matcha memory rises from fifth to third at
    64 KB.
  - The other questions are unchanged.
