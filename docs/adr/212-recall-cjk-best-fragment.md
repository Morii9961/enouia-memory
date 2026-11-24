# ADR-MEM-212 — CJK recall scores each memory's best fragment

Date: 2026-11-24. Status: Superseded by ADR-MEM-213 (lost the 第一次喝酒
memory and tied all 京都 memories on the real library); synthetic validation. Replaces the CJK scoring of ADR-MEM-210 and
211, after the Runtime session's third real-library check.

## Context

The Runtime session checked ADR-MEM-211 on the real library, read-only
(intent `current`, limit 8; the head did not move). It regressed, so Runtime
stayed on ADR-MEM-210:

- **"我去过京都吗".** Two memories were selected. The top one (1803) was about
  Tokyo: it scored on the single character 京, counted three times. None of
  the Kyoto diaries was selected.
- **"摄影集".** "主人喜爱摄影…" (1303) led by repeating 摄影. The memory that
  contains 摄影集 was not selected; under ADR-MEM-210 it ranked first.
- **"我平时喜欢喝什么茶？".** One drink memory was selected, and no memory
  containing 茶.

Counting single characters and repeats let common parts outscore the whole
word. Long memories then lost the rendered-byte budget too, because the
places went to whatever ranked first.

## Decision

For CJK text, the contract matches each run of the query (for example
"我去过京都吗") by fragment:

1. **Fragments.** These are the run's substrings of up to
   `MAX_RECALL_FRAGMENT_CHARS` (8) characters, at most
   `MAX_RECALL_FRAGMENTS` (256) per query. Each weighs
   ⌊log2(N / max(df(fragment), df(its rarest character)))⌋ over the
   candidates, in integers.
   - Question words made of common characters weigh little: 我去过 is held
     to the weight of 去 or 过.
   - A real word keeps the weight of its rarest character: 京都 weighs as
     much as 京.
2. **Strong fragments only.**
   - A fragment of two or more characters needs weight 2, which means it
     occurs in at most a quarter of the memories.
   - A single character needs weight 5, at most 1 in 32 of the memories:
     茶 counts, 京 or 摄 alone does not.
   - A library too small to reach these thresholds uses the heaviest weight
     it has.
3. **Best fragment per run.** A memory scores, for each run, the best of its
   strong fragments by weight × length. It adds nothing for repeats.
   Containing the whole word 摄影集 (3 characters) beats containing 摄影
   (2 characters) any number of times. A memory with no strong fragment and
   no matching word has no cue.
4. **Ranking is unchanged.** It stays lexical ×100 plus alias and attention.
   Whitespace words count 1 each, as before.

`recall_terms` now returns `words` and `runs`. Queries without CJK are
unchanged, so the algorithm stays `attention-rank-1`.

## Verification

`enouia-memory-contract` `attention`, with everyday filler in Morii's voice
that makes 我, 去, 过, 喜欢 and 什么 common:

- **Tea.** "我平时喜欢喝什么茶？" ranks the matcha memory first and leaves out
  an unrelated one.
- **Kyoto.** "我去过京都吗" ranks the four Kyoto diaries first. The Tokyo
  memory, which shares only 京, is not selected.
- **Whole word.** "摄影集" ranks the memory containing 摄影集 above one that
  repeats 摄影 three times.
- **Other queries.** "Enouia迁移", "滑雪" and "Enouia" behave as before.
  The catalog and qualified selectors agree.
- **Against ADR-MEM-211.** The Kyoto and 摄影集 tests fail on 4d5e41f, so
  they capture the regressions Runtime reported. The tea test passes on
  both.
- **Other suites.** The contract, host and init suites pass.
