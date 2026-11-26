# ADR-MEM-222 — The semantic guard rises to 530 and the bonus stops at 4.0 deviations

Date: 2026-11-26. Status: Accepted (route: semantic recall); synthetic
validation, awaiting the Runtime session's real-library rerun. Refines
ADR-MEM-221, which Runtime adopted (Runtime #270).

## Context

Runtime's real-library check of ADR-MEM-221 gave the z of the top records
for each question. z ≥ 3 held for most of them:

- Kyoto diaries stood 3.4–4.5 deviations out.
- 摄影集 stood 4.6–4.7.
- An irrelevant top hit for pets stood 2.98 and stayed out.

Two cases went wrong:

- **Tea.** "我平时喜欢喝什么茶？" has a narrow distribution (mean 394,
  deviation 41). Two irrelevant memories at 517 stood 3.01 deviations out
  and were cued at 64 KB. The lowest similarity of any relevant cue across
  the set was 537 (the university answer).
- **First drink.** A related memory that is not the answer stood 6.32
  deviations out (631). The answer stood 4.84 (568). The related memory's
  bonus was so large that it, and a weakly related one, took the 16 KB
  budget, and the answer dropped out.

## Decision

- **Guard.** `SEMANTIC_GUARD_MILLI` rises from 500 to 530. Below it, a
  similarity adds no score and gives no cue, however far it stands out.
- **Cap.** The bonus stops growing at `SEMANTIC_BONUS_CAP_Z100` (4.0
  deviations). It is at most (4.0 − 2.0) × 100 × 60 = 12000. Memories very
  close in meaning still get the most semantic score, but among them words
  and attention decide.

Nothing else changes.

## Verification

- **Tea.** `semantic_similarity_is_judged_against_its_own_distribution`
  builds a narrow distribution (mean 394, deviation about 40). There, 517
  is about 3.0 deviations out; it would cue with the old guard and now
  gives nothing.
- **Cap.** Similarities of 990 and 1000 earn the same capped bonus.
- **Earlier cases.** The other attention tests pass unchanged.
