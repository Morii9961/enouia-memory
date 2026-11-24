# ADR-MEM-214 — Recall matches titles, and rare words outweigh common characters

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use and Runtime
recall); synthetic validation, awaiting the Runtime session's real-library
check. Refines ADR-MEM-213, which Runtime adopted (Runtime #256).

## Context

The Runtime session checked ADR-MEM-213 on the real library (read-only,
limit 8). It was better than every earlier version, but four issues remained:

- **"我去过京都吗".** No diary was selected within 16 KB. Two causes
  combined:
  - Memories mentioning 京都 once still won through summed common
    characters and chance pairings such as 过京 (from "说过京都").
  - One diary names 京都 only in its title, and recall did not read titles.
- **"摄影集".** A broader memory about 摄影 led the memory containing
  摄影集. Both had the same weighted sum; the broader one won on repeats.
- **"我平时喜欢喝什么茶？".** The matcha memory came second; the third result
  was noise.

## Decision

The recall contract's lexical rule changes as follows.

1. **Titles are matched.** A memory is matched on its content and its title
   (`record_fields.title`), as catalog search already does. This applies to
   every query: a memory named by its title is a fair match in any language.
2. **Question characters split runs.** Sentence particles and interrogatives
   (`RECALL_QUESTION_CHARS`: 吗呢吧啊呀嘛么什怎哪谁几) split CJK runs. They
   are never terms, and a whitespace word made only of them is not a word.
   Memories are statements, where these characters are rare, so weighting
   them by rarity would overrate them.
3. **A piece is a word, or a chance pairing.**
   - A piece is a word when its memories are more than a quarter of its
     rarer character's memories. 京都 holds most of 京.
   - Otherwise it is a chance pairing, weak whatever its weight. 过京 holds
     almost none of 京.
   - Weights stay ⌊log2(N/df)⌋, never above the rarer character, as in
     ADR-MEM-213.
4. **Strong terms decide; weak terms only break ties.**
   - Strong terms are words of weight 3 or more (in at most an eighth of the
     memories) and characters of weight 4 or more. A library too small to
     reach these uses its heaviest weight.
   - A strong word counts up to two mentions. A mention in the title counts
     as two, because a title names what the memory is about.
   - Weak terms, characters already inside a matched piece (counted once),
     and further repeats go to a tie-break of at most 15.
   - The CJK lexical score is strong × 16 + tie-break, so no number of
     common characters outweighs one rare word.
   - A memory with no strong term and no matching whitespace word has no cue.
5. **The whole run earns a bonus.** A memory holding a whole run of three or
   more characters (摄影集) adds that run's heaviest piece once more.

Queries without CJK keep their old terms and counts. They now also match
titles. The algorithm stays `attention-rank-1`; CORE6_RECALL.md states the
rule.

## Verification

`enouia-memory-contract` `attention`, 15 tests, with everyday filler in
Morii's voice:

- **Earlier cases.** Every case from ADR-MEM-210 to 213 still passes: tea and
  matcha, 首次饮酒, the Kyoto diaries against the Tokyo memory, the whole
  word, and repeats as tie-breaks.
- **Titles and chance pairings.** The three Kyoto diaries rank above a
  memory that says 去过 and 说过京都 among common characters. One diary
  names 京都 only in its title. This test fails on ADR-MEM-213 (baf55b6).
- **Whole word.** A memory holding 摄影集 leads one repeating 摄影 four
  times. This synthetic case also passes on ADR-MEM-213: the query word
  itself already separates them. Only the real library can confirm the
  Runtime case.

The contract, host and init suites pass.
