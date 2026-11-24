# ADR-MEM-210 — CJK recall terms are weighted by rarity

Date: 2026-11-24. Status: Accepted; its half-of-best floor is replaced by
ADR-MEM-211 (route: ChatGPT daily use and Runtime recall); synthetic validation. Follows a real-library check by the Runtime
session.

## Context

ADR-MEM-209 counted every CJK two-character piece as one lexical hit. The
Runtime session checked it on the real library, read-only (recall preview,
intent `current`, limit 8; the head did not move):

- **"Enouia迁移".** 3 memories, all on target (none before).
- **"我平时喜欢喝什么茶？".** 4 memories (none before), none about tea.
  - All four scored 203. They matched through common pieces such as "喜欢",
    so attention state decided their order.
  - The library has no tea preference. A literal search for "茶" finds three
    memories (sushi flavours, braces, Uji matcha), but none was selected:
    "茶" is a single character, and the piece "么茶" matches nothing.

## Decision

The recall contract treats words and CJK terms differently. `recall_terms`
returns three lists:

- **Words.** Whitespace words, and the non-CJK parts of mixed words such as
  "enouia". Each counts 1, as before.
- **Pieces.** Each CJK run's overlapping two-character pieces.
- **Characters.** The run's single characters.

Pieces and characters together are capped at `MAX_RECALL_CJK_TERMS` (64),
pieces first.

Over the candidates the selector was given (N memories):

1. **Weight.** A piece or character weighs ⌊log2(N / df)⌋, where df is the
   number of candidates containing it. The arithmetic is in integers, so
   every platform ranks alike. A term in more than half of the candidates
   weighs 0 and is ignored; this drops 我, 的 and similar.
2. **No double count.** A character inside a matched piece is not counted
   again.
3. **Floor.** When any CJK term has weight, a memory scoring under half
   the best score among adopted candidates has no cue (`no_cue`). Common
   pieces alone no longer recall.
4. **Score.** The lexical score is words plus weights. Ranking stays
   lexical ×100 plus alias and attention, so a memory sharing rarer terms
   ranks first.

A query without CJK has no pieces or characters. Its terms, scores and floor
(0) are exactly as before, so traces and goldens for such text are
unchanged, and the algorithm stays `attention-rank-1`.
[CORE6_RECALL.md](../contracts/CORE6_RECALL.md) states the rule.

Weights depend on the candidate set, so the same query can rank differently
as the library grows. Each trace still records the snapshot it ranked.

## Verification

`enouia-memory-contract` `attention`:

- **Term lists.** `recall_terms` returns the three lists.
  - Spaced Chinese ("喝 茶 喝茶") stays words.
  - Full-width punctuation is not a character.
  - Long text stays within the cap.
- **Selection.** In a 16-memory synthetic library where "喜欢" is common and
  one memory mentions 抹茶, the catalog and qualified selectors agree:
  - "我平时喜欢喝什么茶？" ranks the matcha memory first and leaves out the
    three memories that share only "喜欢";
  - "Enouia迁移" and "寿司" each select their one memory;
  - "滑雪" selects nothing;
  - "Enouia" (no CJK) selects as before.

`remote_daily_process` still recalls the Chinese memory through ChatGPT. The
contract, host and init suites pass.
