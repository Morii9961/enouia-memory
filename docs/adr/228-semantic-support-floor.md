# ADR-MEM-228 — Words alone below a supported result are left out

Date: 2026-11-27. Status: Candidate, awaiting Runtime's real-library
comparison. Extends ADR-MEM-220/221 (semantic recall).

## Context

Runtime probed its real library (351 memories, bge-m3 vectors ready,
limit 8) with eleven questions at 16 KB and 64 KB on a361404 and judged
each selected memory related (R), partly related (P) or unrelated (N).
At 64 KB, of 70 selected memories:

- 19 were matched by words and meaning (reasons `lexical` and
  `semantic`): 12 R, 7 P, none N;
- 51 were matched by words alone: 2 R, 14 P, 35 N.

On questions with an answer, the words-only memories ranked below the last
memory with both reasons were nearly all unrelated (drawing style, first
drink, breakfast, Kyoto, photo book). Two questions with no answer in the
library (tea, sport) had no semantic match at all and returned eight
unrelated memories. The university question also had no semantic match,
but its results were related, and its scores were within 100 of the sport
question's, so no score threshold separates the two.

## Decision

- After ranking, the selector takes the top `limit` candidates. If any of
  them has both `lexical` and `semantic`, the lowest score among those is
  the floor. Every candidate without `semantic` scoring below the floor is
  left out with the new exclusion reason `unsupported`. It is listed after
  `budget` and before the filtered reasons (ADR-MEM-227).
- The floor does not depend on the byte budget, so 16 KB and 64 KB leave
  out the same memories.
- A memory found by meaning alone sets no floor and is never left out by
  it. Without vectors (the ChatGPT connector today) nothing changes.
- A recall may now return fewer than `limit` memories with budget to spare.
- Questions with no semantic match (tea, sport, university) are unchanged.
  Telling "no answer" from "answer found by words only" needs another
  signal; this ADR does not try.

Applied to Runtime's 64 KB results, this rule leaves out 16 memories:
12 N, 4 P, no R (breakfast 2, drawing style 6, first drink 6, photo book 2).
Runtime simulated a broader variant in which a meaning-only memory also sets
the floor: it left out 21, including the one related words-only memory of
"what am I learning lately", so that variant is not used.

## Contract

`RecallExclusionReason::Unsupported` (`unsupported`) is added to the
exclusion enums of the read-model and the three Agent read schemas, and to
`reason_counts`.

## Verification

`words_alone_below_a_supported_result_are_left_out` (contract): with
vectors, a words-only memory below a words-and-meaning one is left out as
`unsupported` and a meaning-only memory is kept; with meaning alone, or
without vectors, the words-only memory stays.
