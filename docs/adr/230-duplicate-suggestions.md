# ADR-MEM-230 — Possible duplicates, for Morii to review

Date: 2026-11-27. Status: Accepted (IMPLEMENTATION_PLAN §15 "smart
tidying", started at Morii's request of 2026-11-27); synthetic validation.

## Context

The plan allows smart tidying that suggests duplicates, never changes an
identity automatically and never bypasses audit. The ChatGPT connector
records a day's conversation every day, so the same fact or preference will
be restated across days. Revisions of one memory are already linked by
lineage (`derived_from`, `reverts_to`) and are versions, not duplicates.

## Decision

A read-only System projection on `SegmentedMemoryReader`,
`memory_duplicate_suggestions_native(request, query)`, rebuilt from the
freshly authorized set, like ADR-MEM-164's review suggestions. It writes
nothing.

- **Candidates.** The catalog's selection, filter and sensitivity ceiling.
  At most `MAX_DUPLICATE_CANDIDATES` (2000) memories are compared pairwise;
  `complete_in_scope` says whether all selected ones were.
- **Same wording.** The lowercase letter-and-digit character pairs of the
  two contents overlap by at least 600 thousandths (Jaccard). Contents
  with fewer than four pairs are too short for this signal. A restatement
  that keeps most words passes; a looser paraphrase (about a third shared
  in the synthetic case) does not and is left to meaning.
- **Same meaning.** With `query.semantic` and an attached index, the two
  vectors' cosine is at least 900 thousandths. On Runtime's real library the
  closest distinct pairs (two exams of one course, two migrations) stayed at
  or below 830.
- **Not a duplicate.** A pair in one revision lineage (one derived from or
  reverting to the other) is left out.
- **Suggestion.** Each pair lists the earlier and the later memory (title,
  capture time), its reasons, the wording score and the semantic score when
  compared. Pairs with both reasons come first, then the higher score.
  `total_pairs` counts all; `items` holds the first within `limit` and
  `max_bytes`.
- **Resolution.** Morii decides. Keeping one and withdrawing, forgetting
  or correcting the other uses the existing Owner actions; after that the
  pair is gone from the next page. A "not a duplicate" answer is not
  recorded yet, so a dismissed pair shows again; that needs its own Owner
  action and is left until Morii uses the list.

The types are host-level structs in `automation::catalog::duplicates`. No
contract schema, persisted format, IPC command or wire change.

## Verification

`possible_duplicates_are_suggested_by_wording_or_meaning` (host): five
memories. Without vectors only the near-verbatim Chinese pair is suggested
(same wording, earlier first); with vectors the English sentence that means
the same is paired with both, the both-reason pair leads, and a weather
note and a one-character memory are not suggested; with `semantic: false`
the vectors are ignored; deleting one copy removes the suggestion.

## Runtime's real-library check (2026-11-27, bcaa77b)

Read-only on the real library (351 memories, vectors ready 351/351):

- `total_pairs` 0 with and without `semantic`, `complete_in_scope` true,
  about 120 ms.
- Over all 61,425 pairs: the highest cosine was 831 (16 pairs at or above
  800, none at or above 850); the highest wording Jaccard was 188.
- Runtime judged the twelve closest semantic pairs: 11 unrelated as
  duplicates (the same topic at different times: two sleep records, a
  midterm and a final, two migrations) and one partly related (a decision
  and its sign-up details on the same day). None was a duplicate.
- So the library has no duplicates yet and neither threshold raised a false
  one. The wording signal catches only near-verbatim copies; long contents
  share few character pairs. Runtime does not show the list until there is
  something in it.
- A `limit` outside 1–100 fails with the generic `Invalid`, as every catalog
  read does.
