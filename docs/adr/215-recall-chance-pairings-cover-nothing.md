# ADR-MEM-215 — A chance pairing does not cover its characters

Date: 2026-11-25. Status: Accepted; the real-library check showed no change
in any order (the 摄影集 tie had another cause, see ADR-MEM-216), so Runtime did
not adopt it on its own. Refines ADR-MEM-214, which Runtime adopted (Runtime #258).

## Context

ADR-MEM-214 passed the Runtime session's real-library check at both budgets.
One ordering stayed wrong:

- "摄影集" ranked the broader memory first, "主人喜爱摄影" at 16603.
- The memory containing 摄影集 came second, at 16403.

Both had strong score 10, so the tie-break decided. Two rules together
produce that tie:

- **影集 is a chance pairing.** In the library, 集 appears in many words,
  and 影集 holds a quarter or less of its memories, so it is weak.
- **A matched piece still covered its characters.** The 摄影集 memory's 集
  was therefore not counted as a character either.

The broader memory counts its own, unrelated 集 as a strong character, which
evens the two scores.

## Decision

Only a word, a piece that is not a chance pairing, covers its characters.
The characters of a matched chance pairing are counted like any other
character: strong at weight 4, weak otherwise. A memory holding the whole
query word therefore keeps every character it shares with the query.

Nothing else changes. Queries without CJK are unchanged.

## Verification

`enouia-memory-contract` `attention`: all 15 tests pass. The whole-word test
now gives the 摄影集 memory a title, as in Runtime's case.

The synthetic library cannot reproduce the real character frequencies. Here
集 is rarer than 影, so 影集 counts as a word, and the test passes with or
without this change. The real-library check decides.

The contract, host and init suites pass.
