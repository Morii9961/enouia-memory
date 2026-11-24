# ADR-MEM-211 — CJK recall keeps every memory with a strong term

Date: 2026-11-24. Status: Superseded by ADR-MEM-212 (it regressed on the
real library); synthetic validation. Follows the Runtime session's second
real-library check.

## Context

ADR-MEM-210 weighted CJK terms by rarity, and excluded any memory scoring
under half the best score. The Runtime session checked it on the real
library, read-only (intent `current`, limit 8; the head did not move).

**Better than before:**

- "我第一次喝酒是什么时候？" selected exactly the memory about the first
  drink.
- "摄影集" and "Enouia迁移" were reasonable.
- "我平时喜欢喝什么茶？" now selected memories about drinks.

**Still wrong:**

- "我去过京都吗" selected three memories, and the top two were unrelated. A
  literal search for "京都" finds 12 adopted memories, nine of them
  Japan-trip diaries, and none was selected. Two effects caused this:
  - A stray memory hit rare chance pairings such as 我去 and 去过. That
    raised the best score.
  - The half-of-best floor then cut every memory that only hit 京都, the
    word actually asked about.
- The Kyoto matcha memory still missed the tea question in the same way.

A floor relative to the best score lets one stray memory push out every real
answer. Rarity alone also overrates chance pairings: 过京 or 去过 can be
rarer than any real word in the question.

## Decision

In the recall contract, for CJK terms only:

- **Piece weights are bounded by their characters.** A piece weighs
  ⌊log2(N / max(df(piece), df(rarer character)))⌋.
  - A chance pairing across words (过京, 我去, 去过) is never rarer than its
    rarer character.
  - A real word such as 京都 or 喜欢 keeps its weight, because its
    characters mostly occur in it.
- **Repeats count, up to three.** A piece or character adds its weight once
  for each occurrence in the memory, at most three times. A memory that
  keeps returning to the word asked about (a trip diary) ranks above one
  that mentions it once.
- **No relative floor.** The half-of-best cut is removed. A memory is cued
  by CJK terms only when it hits a strong term: weight 2 or more (in at most
  a quarter of the memories), or, in a library too small to reach 2, the
  heaviest weight any term reaches. Whitespace words still cue as before.
- **Ranking is unchanged.** It stays lexical ×100 plus alias and attention.
  The profile's item limit bounds the result, so common pieces such as 喜欢
  can only fill places after the memories sharing rarer terms.

Queries without CJK are unchanged, as in ADR-MEM-209 and 210.

## Verification

`enouia-memory-contract` `attention`. Everyday filler memories in Morii's
voice make 我, 去 and 过 common, as in a real library.

- **Tea.** "我平时喜欢喝什么茶？" ranks the matcha memory first. Memories
  sharing only 喜欢 come after the drink memory, if at all.
  - "Enouia迁移" selects its one memory.
  - "滑雪" selects nothing.
  - "Enouia" (no CJK) behaves as before.
- **Kyoto.** "我去过京都吗" selects all six Kyoto diaries. A stray memory
  that says 去过 twice and 京都 once does not rank first.
- **Other suites.** The catalog and qualified selectors agree throughout.
  The contract, host and init suites pass.
