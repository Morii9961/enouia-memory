# MCP-3 durable native Agent hints

2026-11-03. Base `237c60d`. [ADR116](../adr/116-durable-agent-note-intake.md)
and [intake contract](../contracts/AGENT_NOTE_INTAKE_V1.md).

Native submission now saves an optional Agent hint in the existing witnessed
Owner chain. Its own `hint_saved` receipt is distinct from source capture, queue
and formed-memory receipts. Exact original requests replay original receipts;
changed content under the same principal/request key conflicts. Preparation is
reused, then publication requires the same snapshot and a live delegation. No
draft is accepted as authority, and replay still requires current source access.

The new closed delta validates Agent provenance, exact original evidence,
current policy/source, key uniqueness, Owner layout and capacity during both
publication and chain replay. Notes are not source or memory changes in process
projections. Existing Agent reads gain no write tool. Catalog schema fingerprints
change, so matching native client/server revisions are required. Old-binary
refusal is inferred from closed enum decoding; no old binary was executed.

Focused checks: 1 contract corpus test and 7 Host note tests passed, with no
failures. They cover the original three preparation regressions plus saved
receipt/reopen identity, changed-key payload conflict, current deletion barriers,
32-hint capacity with existing-receipt replay, real two-thread duplicate replay,
and publication faults both before and after CURRENT. Unknown publication retains
the exact original request and blocks replacement packets. Recovery plus explicit
fresh native setup yields one receipt without automatic replay or model calls.
Source count stays unchanged, and no queue or memory is created.

Final offline/locked serial workspace run: **887 passed, 0 failed, 4 ignored**,
across 107 test/doc-test targets, exit 0. Fmt, strict workspace/all-target Clippy,
Runtime surface and whitespace checks passed on the final implementation.
Aggregate: `de93018f21667885f38b69319c692ecbabe4df4d476f8f7f7d3838ac7b8c6ae0`.
Ignored local logs: target/note-intake-workspace-tests.log and
target/note-intake-workspace-clippy.log. No later implementation changes were
made after that run started. Fifth-slice preparations stayed outside the checkout.

The independent pinned Python jsonschema 4.26.0 cross-check passed all 65 schemas,
including 16 new note record/receipt/delta cases and the existing fixture corpus.
The first Rust corpus attempt caught a synthetic key computed using compact JSON;
the repository uses pretty canonical JSON plus newline. The fixture was corrected
to that existing format, then the Rust corpus passed. No implementation workaround
or dependency change was made.

Owned files: contract note types/module, segmented delta/schema and new note
schema/test corpus; Vault note intake, Agent preparation, state/replay/exports
and process change classification; Host native writer and note tests; Python
cross-check, ADR116/register, both note contracts, this report, Runtime log and
generated surface. New synthetic roots and fake/native local paths only. No
private Vault, real analyzer, Runtime edit, actual MCP client or production use.

System hint consumption, explicit MCP note write negotiation and real model
quality remain later work. Saving a hint does not promise its consumption or
trigger another analysis. No standalone hint journal or human review queue exists.
