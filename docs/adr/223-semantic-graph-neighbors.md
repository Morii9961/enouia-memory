# ADR-MEM-223 — The System graph lists semantic neighbors from an attached index

Date: 2026-11-26. Status: Accepted (route: semantic recall); synthetic
validation, awaiting Runtime's adoption. Extends ADR-MEM-220/221 from
recall and the Agent's `related` to the System graph page.

## Context

Since ADR-MEM-220, recall and `related` can use a `SemanticIndex`. The
System graph page (`memory_graph_native`, the star map in Runtime) still
reported `semantic_neighbors_available: false` with the note
`semantic_neighbors_unavailable_no_model_or_embeddings`. Its schema fixed
that field to false. Runtime now has bge-m3 vectors for the real library
(about 350 memories).

## Decision

- **When.** A reader with an index (`with_semantic`) lists semantic
  neighbors in the graph when 2 to `MAX_SEMANTIC_GRAPH_VECTORS` (2000) of
  the selected nodes have a vector of the index's model. Otherwise the
  page reports none as before. The graph compares every pair of those
  vectors: 2000 vectors are about two million pairs. Runtime's library
  needs about 60 thousand.
- **Which pairs.** Each node's similarities to the other selected nodes
  form its own distribution. It keeps the ones that would cue in recall
  (`SemanticStats::score`, ADR-MEM-221/222), at most `MAX_SEMANTIC_NEIGHBORS`
  (5), closest first. This is the same rule as `related`. A pair either
  end keeps becomes one undirected edge, ordered by similarity.
- **Edge.** `semantic_neighbor`, class `algorithm_suggestion`, witness
  `semantic:<model>`, `score_milli` the cosine in thousandths. Edges count
  against `edge_limit_per_kind`, and coverage gets a `semantic_neighbor`
  entry with exact candidates and retained counts.
- **Page.** `semantic_neighbors_available` is true exactly when that
  coverage entry is present. The header note becomes
  `semantic_neighbors_are_model_similarity_suggestions`.
- **Contract.** The schema allows true. The native check requires that
  `semantic_neighbors_available` matches the coverage entry, that no
  semantic edge appears without it, and that a semantic edge is an algorithm
  suggestion with a score. Coverage may have 8 kinds.

A vector that appears or disappears between two pages changes the header
note, so an old cursor is stale and the caller starts over, as with any
other change in scope.

## Verification

- `the_graph_lists_semantic_neighbors_from_an_attached_index` (host): two
  memories with cosine 0.9 become one semantic edge with exact coverage and
  the note; a memory without a vector is skipped; without an index the
  page is unchanged.
- Projection corpus: `graph-phantom-vectors` (available without coverage)
  and `graph-semantic-edge-without-vectors` now pass the schema and are
  rejected by the native check.
