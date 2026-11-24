# ADR-MEM-209 — Recall terms split CJK runs in the shared contract

Date: 2026-11-24. Status: Accepted, scoring refined by ADR-MEM-210 (route:
ChatGPT daily use and Runtime recall); synthetic validation. Requested by the Runtime session after a
read-only check on the real library.

## Context

ADR-MEM-208 fixed Chinese questions for ChatGPT only, by expanding the query
text in the connector. Every other entry point still passes Morii's raw text
to the shared recall contract:

- Runtime's recall preview;
- Context;
- local Claude Code or Codex through the MCP bridge.

The Runtime session ran these queries on the real library (recall preview,
intent `current`, limit 8). It read only; the head did not move:

| Query | Selected |
| --- | --- |
| "我平时喜欢喝什么茶？" | 0 |
| "喝 茶" | 2 |
| "Enouia迁移" | 0 |
| "Enouia 迁移" | 3 |
| "摄影集" (a single word) | 3 |

A tokenizer in Runtime alone would duplicate the rule and miss the MCP path.

## Decision

- **Contract.** `recall_terms(text)` in the contract's attention module
  builds the lexical terms that `select_recall`, `select_catalog_recall` and
  `select_qualified_catalog_recall` count. The terms, lowercased, are:
  - every whitespace word, as before;
  - for each run of kana, CJK ideographs or Hangul, its overlapping
    two-character pieces;
  - in a mixed word, its other parts with at least two letters or digits
    ("Enouia迁移" adds "enouia" and "迁移").

  Duplicates count once. Ranking stays lexical count ×100 plus alias and
  attention, so memories sharing more pieces rank first.
- **Unchanged for words without CJK.** Their terms are exactly as before.
  Existing traces and goldens for such text are unchanged, so the algorithm
  stays `attention-rank-1`. CORE6_RECALL.md states the term rule.
- **Connector.** The connector-side expansion of ADR-MEM-208 is removed.
  `memory_recall` and the `session_start` topic now pass raw text, and the
  contract applies the rule once.
- **Accepted noise.** A common piece such as "什么" can let an unrelated
  memory in. It ranks below memories that share more pieces, and the item
  limit bounds the result.

## Verification

`enouia-memory-contract` `attention` tests:

- **Terms.** `recall_terms` returns:
  - "moonlight pottery" unchanged;
  - "摄影集" → the word, "摄影" and "影集";
  - "Enouia迁移" → the word, "enouia" and "迁移".

  Single characters stay words, and duplicates count once.
- **Selection.** On four synthetic memories, the catalog and qualified
  selectors agree:
  - "我平时喜欢喝什么茶？" ranks the oolong-tea memory first and leaves out
    the hiking one;
  - "Enouia迁移" selects only the migration memory;
  - a query with no shared piece selects nothing.

`remote_daily_process` still recalls the Chinese memory through ChatGPT's
`memory_recall` and `session_start`, now via the contract. The host and init
suites pass.
