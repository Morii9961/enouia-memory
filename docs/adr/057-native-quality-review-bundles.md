# ADR-MEM-57 — Native quality review bundles

Date: 2026-10-13. Scope: MV-7-H2h, assessment preparation only.

Add history_quality::build to connect an owner-selected saved semantic task to the
offline human assessment tool. Revalidate the completed response, selected sources
and permissions; retain exact selected spans, role/class, hashes, source context
and uncited coverage. Return a bounded one-case suite in memory; never save/export,
send, propose or approve. The host separately handles any explicit save.

Use native_selected_spans_v1 inline scope, without inventing original conversation
or message IDs. Bind source scope and response value with sorted compact UTF-8 JSON
hashes matching Python; keep native task/input/dispatch and exact response-text hash
separate. Numeric fields are integers. Bundle <=256 KiB; reviewer-specified required
units are unique, 1..32, each <=256 UTF-8 bytes. No unit importance is model-inferred.

The reviewer is pending and both judgment arrays are empty. Empty model responses
retain selected coverage and unknown units; they do not establish completeness.
Assessment accepts pending only without judgments, and still requires explicit
owner/synthetic_fixture judgment labels to claim a reviewed result. Inline sources
cannot also select a source file; snippet byte bounds/hash and duplicate spans are
checked. The offline tool never authenticates declared native validation or owner
metadata; native_validation remains not_rerun and overall_model_quality remains null.

No new stored/wire schema, dependency or command. Real samples/Provider activation
remain separately authorized, and no private bundle is committed. Runtime owns the
product UI. Synthetic Rust-to-Python evidence establishes interoperability only.
