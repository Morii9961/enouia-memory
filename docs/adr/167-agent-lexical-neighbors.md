# ADR-MEM-167 — Lexical neighbor suggestions for Agent readers

Date: 2026-11-23. Status: Accepted (roadmap item, Morii 2026-11-19); synthetic validation.

## Context

The roadmap (IMPLEMENTATION_PLAN §15) asks for bounded, explainable neighbor
suggestions for both System read-only readers and Agent readers. These are
labeled as suggestions, not factual edges, and pass the withdrawal and deletion
filters.

ADR-MEM-162 added `lexical_neighbor` edges to the System graph projection. The
Agent `memory_related` tool, `QualifiedAgentReader::related`, still listed only
recorded relations and theme suggestions.

## Decision

The ADR-MEM-162 computation becomes one shared function, used by both readers.
It has the same terms, document-frequency filter, rarest-first work budget,
two-shared-term and Jaccard 0.15 thresholds, and three-neighbor cap.

- **System graph.** It uses the shared function over the selected nodes, as
  before.
- **Agent `related`.** It anchors on the requested memory. Candidates are all
  authorized current memories in the reader's scope. Term statistics come from
  that set, and only pairs that include the anchor are scored. The anchor's best
  three become `suggested_edges` of relation `lexical_neighbor` with class
  `algorithm_suggestion`. Each edge has a `score_milli` and a
  `shared_terms:` witness.
- **Coverage and labels.** `edge_coverage` gains the `lexical_neighbor` kind, so
  the related schema allows five coverage items. `unavailable` still lists
  `semantic_neighbors`. The Agent tool descriptions add that lexical neighbors
  are term-overlap suggestions.
- **Witness length.** A witness now stops before 256 characters, the read
  schemas' limit, which a very long ASCII term could otherwise exceed.

Out-of-scope, withdrawn, forgotten and deleted memories are not in the authorized
set, so they never appear as neighbors and never contribute term statistics.

## Consequences

- Claude Code, Codex and other Agents see why two memories may be related, as a
  labeled suggestion. They are not told that the memories are linked.
- No persisted format, IPC command or protocol change. The related output now
  carries a fifth coverage entry and may carry `lexical_neighbor` suggested
  edges, which the schemas already enumerated.

## Verification

- **Agent related.** A synthetic library of four memories is used. The anchor's
  only lexical neighbor is the overlapping memory, with a witness containing
  `alps`, a score of at least 150, coverage 1/1 and five coverage entries. The
  unrelated and Chinese memories get none. A reader whose scope excludes the
  neighbor, and the full reader after the neighbor is deleted, both show no
  lexical edge.
- **Regression.** The System graph lexical test and timing probe are unchanged:
  351 long memories take about 1.5 s in debug. The qualified, initialized,
  opening, projection and catalog tests and the Python schema cross-check pass.
