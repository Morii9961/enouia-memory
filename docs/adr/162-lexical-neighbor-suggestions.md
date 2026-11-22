# ADR-MEM-162 — Lexical neighbor suggestions in the graph projection

Date: 2026-11-22. Status: Accepted (roadmap item, Morii 2026-11-19); synthetic validation.

## Context

The roadmap (IMPLEMENTATION_PLAN §15) commits to neighbor suggestion edges in the
graph. On the real library the graph showed only same-conversation edges. The
projection declared semantic neighbors unavailable because Memory runs no model
and stores no embeddings.

## Decision

Add `GraphEdgeKind::LexicalNeighbor`, always `class: algorithm_suggestion` with a
`score_milli`. `memory_graph_native` computes it over the selected, already
authorized nodes:

- **Terms.** Each memory's lowercased title and content give a term set: ASCII
  words of three or more letters and/or digits, plus adjacent-character bigrams
  of CJK ideographs (U+4E00–U+9FFF). A set is capped at 512 terms.
- **Filtering.** Terms that occur in only one node, or in more than
  `max(2, n/2)` of the `n` selected nodes, are dropped.
- **Scoring.** A pair qualifies with at least two shared kept terms and a
  Jaccard score of at least 0.15 over the kept terms; `score_milli` is
  Jaccard × 1000.
- **Bounding.** Pairs are taken by descending score, and each node keeps at
  most three neighbors. Kept terms are taken rarest first (then by term) while
  their pair work stays within two million posting steps; more common terms are
  dropped instead of failing the page. The usual per-kind `edge_limit_per_kind`
  and coverage counts apply.
- **Explanation.** `witness` is `shared_terms:` followed by up to five shared
  terms, rarest first.

The header adds the note `lexical_neighbors_are_term_overlap_suggestions`.
`semantic_neighbors_available` stays false and the "no model or embeddings" note
stays, so a term-overlap suggestion is never presented as a semantic or
recorded relation. Only authorized current memories are nodes, so withdrawn,
forgotten and deleted memories never appear. The contract accepts six coverage
kinds, and `algorithm_suggestion` with a score for `lexical_neighbor`. The four
schemas that enumerate edge kinds add `lexical_neighbor`.

## Consequences

- Runtime can draw these edges as suggestions with their shared terms.
  Every graph page can contain the new kind, so Runtime code that matches edge
  kinds must handle it when the pin moves.
- Real semantic neighbors (model or embeddings) remain a later, separately
  authorized step.
- No persisted format or command change.

## Verification

Synthetic library with four memories:

- two overlapping English memories (Alps, sister, summer, hiking);
- an unrelated English memory;
- a Chinese memory.

The graph has exactly one lexical-neighbor edge, between the two overlapping
memories. It is an algorithm suggestion with a score of at least 150 and a
witness that includes `alps`. Coverage is 1/1 and semantic neighbors stay
unavailable. Existing projection, catalog, process and Agent read tests pass.

## Addendum (2026-11-22): long texts

Runtime measured the graph on a 351-memory real library: 2.9 s in release for 100
nodes, against 43 ms before this ADR. The cost was in the pair stage: every
candidate pair re-counted both nodes' kept terms through string-keyed map
lookups, and shared terms were accumulated in a map per pair.

- **Interning.** Terms are now interned to integer ids.
- **Counting.** Shared terms are counted per node with a reused array, and the
  shared-term witness is computed only for retained edges.
- **Extraction.** Lowercasing is per character over title and content, ASCII
  only, which is equivalent for ASCII words and CJK bigrams. It stops at the
  512-term cap instead of lowercasing the whole text first.
- **Budget.** The two-million-step budget now drops the most common terms (see
  Bounding) instead of returning `Capacity`.

The rules and the output for the same kept terms are unchanged.

A synthetic timing probe (ignored test
`lexical_neighbors_on_long_texts_stay_fast`) uses 351 memories of 3,000
characters each with a skewed vocabulary. In debug builds, 100 nodes went from
2.7 s to 0.42 s and 351 nodes from 231 s to 1.3 s.
