# ADR-MEM-82 — Workspace local history navigation brief

2026-10-23. Expose the existing deterministic ADR-MEM-52 helper for the same
explicit saved task/batch selection used by ADR-MEM-81 comparison.

Add read-only history_brief with batches, comparisonCommitId and maxBytes.
Selections use the same 1–32 distinct native-resolved admission rule. The
observed comparison head must still be current throughout task resolution,
brief generation and result construction. Reject stale/locked/revoked reads.

Return base, native-resolved batches, contentHash, markdown and maxBytes.
contentHash is SHA-256 of the exact UTF-8 markdown bytes. maxBytes is 1 through
2 MiB; the caller normally chooses 128 KiB. Reuse the helper's bounded writer:
overflow refuses the whole result. The wire exposes all selected topics only;
an arbitrary caller topic/text/filter or output path is not accepted.

Preserve all selected source coverage, including uncited/zero-candidate gaps.
The helper's Chinese navigation text and JSON blocks are a deterministic dossier,
not a model-generated summary, proof of completeness, Canonical source or
instruction. Runtime displays the exact text inertly and validates the hash,
selection and snapshot before publication. It must recheck lifecycle/selection
after asynchronous hashing and clear previous text before refresh.

No file export, clipboard copy, Provider request/setup, automatic follow-up,
candidate mutation or approval, Host/MCP/cloud activation or stored schema is
introduced. Runtime product entry remains Claude's responsibility.
