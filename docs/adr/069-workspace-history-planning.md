# ADR-MEM-69 — Read-only archived history planning in workspace IPC

Date: 2026-10-19 (Asia/Shanghai).
Status: Owner-authorized MV-7 integration slice; implemented with local verification and explicit suite-concurrency limits below.

The H1 planner and later history helpers remain optional native libraries. Runtime cannot host the planner by reading Vault files or reaching around Workspace. The owner explicitly authorized a first cross-repository read-only planning slice, followed by Runtime pin adoption and UI. This does not activate MV-8, real history import, model calls or cloud services.

## Decision

Add `history_plan` to workspace IPC v1 as command 48, returning kind `history_plan`, with a null idempotency key. It accepts a current import ID/revision, explicit messages-per-batch (1–15) and path-reference budget (1–100,000), plus a cursor and page limit (1–20, default 20). It names no file, model, actor or endpoint. Hosts continue deriving `HostSurface` from native identity: the main workspace may forward it and Quick Search may not.

Workspace reads a completed, nonduplicate archived input through the Vault with a 64 MiB received/decompression bound and the same adapter. It refuses schema/member drift. Every parsed message, including undated/unsafe-graph messages, must still bind to a current readable Source revision by locator, raw/content hashes, kind, role/evidence, time/completeness and upstream IDs. Missing, revised or deleted sources are refused; rereading Raw cannot bypass source deletion. The ordinary current SourceRead policy is evaluated for inspection preview, with no egress destination or grant.

Results contain the import/adapter/Raw binding, the observed Vault/head, coverage counts, observed time range, unknown time and attachment/parse gaps, issue summaries and paged batch/source references. They contain no source text or raw locator, and never claim complete platform history. Up to 100 issues are shown with an explicit omitted count. The parsed-input cap is 100,000 messages/units; serialized results are capped at 128 KiB. Oversize inputs/results are refused rather than silently truncated.

The plan hash binds the parameters, archive/revision, current head and an in-memory nonce for this Vault opening. Cursors bind that hash and offset; a head change, changed parameters, another opening/unlock or another Vault makes them stale. A final head observation refuses a detected concurrent mutation. The existing Core lifecycle guard excludes lock/switch/shutdown while handling the read. Frontends must still discard observations after lifecycle/selection changes; any future materialization must independently validate current sources and permissions.

Planning creates no Session, extraction job, invocation, candidate, canonical memory or persisted plan. Batch indices/export branch selection are navigation, not approval or permission to extract. H2 materialization and semantic/review operations are later separate integrations.

## Adoption gates

Memory's required Rust tests, formatting, strict Clippy, independent schema checks and regenerated/logged Runtime surface must pass before reporting this slice complete. Runtime then adopts an exact pushed revision, updates its rendered-field/command checks and uses only the workspace seam. Real-data/quality/performance and Morii acceptance remain pending. See [implementation evidence](../validation/MV-7-workspace-history-planning.md).
