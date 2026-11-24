# ADR-MEM-213 — CJK recall sums weighted terms, with repeats as tie-breaks

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use and Runtime
recall); synthetic validation. Replaces the CJK scoring of ADR-MEM-212,
after the Runtime session's fourth real-library check.

## Context

Over four checks on the real library (read-only, intent `current`, limit 8,
budget 16384 bytes), the Runtime session compared each version's scoring:

| Question | 210 (summed terms, half floor) | 211 (repeats counted) | 212 (best fragment) |
| --- | --- | --- | --- |
| 我第一次喝酒是什么时候？ | right memory | right memory | lost: single characters needed weight 5 |
| 摄影集 | right memory first | lost to repeats of 摄影 | right memory first |
| 我去过京都吗 | diaries cut by the floor | Tokyo memory led on 京 ×3 | all 京都 memories tied at 603, budget picked 3 |
| 我平时喜欢喝什么茶？ | drink memories | worse | worse |

The pattern was consistent across versions:

- **Summing terms works best (210).** Its only real failure was the
  half-of-best floor.
- **Counting repeats in the score misfires (211).** It let a part of a word
  outscore the whole word.
- **Best-fragment-only scoring is too strict (212).** It drops a memory
  worded differently, such as 首次饮酒 for 第一次喝酒, and it ties every
  memory that holds the same word.

## Decision

The CJK score returns to ADR-MEM-210's sum and keeps what worked since:

1. **Terms.** Each CJK run of the query gives its two-character pieces and
   its characters. Together they are capped at `MAX_RECALL_FRAGMENTS` (256).
2. **Weights.** A term weighs ⌊log2(N / df)⌋ over the candidates, in
   integers.
   - A piece weighs no more than its rarer character, as in ADR-MEM-211.
     This holds 我去 and 去过 down, while 京都 keeps the weight of 京.
3. **Score.** Every matched term adds its weight once. A character inside a
   matched piece is not counted again.
4. **Cue.** A memory is cued through any of:
   - a matched piece of weight 2 or more;
   - a matched character of weight 4 or more (at most 1 in 16 of the
     memories), so 酒 or 茶 can find a memory;
   - a whitespace word, as before.

   There is no relative floor. A library too small to reach these weights
   uses the heaviest it has.
5. **Ties.** The CJK lexical score is the sum × 4, plus the memory's
   repeats of matched pieces, capped at 3. Repeats only order memories that
   share the same terms; they never outrank a stronger match. Ranking stays
   lexical ×100 plus alias and attention, so for CJK queries an alias weighs
   less against a lexical unit than before.

`recall_terms` keeps `words` and `runs` from ADR-MEM-212. Queries without
CJK are unchanged, so the algorithm stays `attention-rank-1`.

**Not addressed: titles.** Recall matches memory bodies. A memory that names
the topic only in its title (one Kyoto diary) is still not found. Adding
titles to recall candidates is a separate change.

## Verification

`enouia-memory-contract` `attention`, with everyday filler in Morii's voice.
The earlier scenarios all still pass:

- **Tea.** The matcha memory leads the tea question.
- **Kyoto.** The Kyoto diaries lead "我去过京都吗", and the Tokyo memory is
  not selected.
- **Whole word.** The 摄影集 memory leads "摄影集".

New:

- **Different wording.** "我第一次喝酒是什么时候？" leads with the memory
  that says 首次饮酒, through 酒. Memories that share only 第一次 do not
  outrank it.
- **Ties.** For "京都", the diary returning to 京都 ranks above a passing
  mention. For "京都抹茶", the memory that also has 抹茶 leads whatever the
  repeats.

The contract, host and init suites pass.
