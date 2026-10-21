# MV-7 · Prepare one saved history batch

Date: 2026-10-20. Baseline `a6e4da9958730ac8d7a151ce49c1108ff80f4228`.
Scope: [ADR-MEM-71](../adr/071-history-batch-request-preparation.md).

Current main already implements paged history planning and recoverable local
materialization (PRs #37/#38), and Runtime records adoption of that revision.
The earlier native-only planning draft PR #36 remains separate and was not
merged or copied into this slice. This work continues the current 52-command
baseline without changing Runtime or the manual status snapshots.

## Contract and behavior

IPC v1 adds keyed synchronous `history_batch_prepare` (53 commands). Arguments:
`taskId` (saved admission Session), `batchIndex` (original selected unsigned
32-bit index), `policyId` (existing egress policy), `outputTokens` (1–32,768).
The page cannot provide a run, child Session, input event, API/model, actor,
source, endpoint or filesystem path. Main-window identity remains native;
Quick Search refuses this command under the shared HostSurface rule.

The Core revalidates the original admission/import and all selected source
bindings, resolves the selected existing child from canonical receipts, and
requires ready/uninvoked state and the configured exact saved model. No read
repairs missing records; an absent child requires explicit local resume first.
An existing invocation of any outcome is not a request to retry the model.

Success returns the usual `external_prepared` result: exact body, endpoint,
wireHash, expiring callId, inputEventId, dispatchId, API/model and output ceiling,
plus `taskId`, `batchIndex`, `sessionId` and `runId`. No model call or credential
read occurs. A local capsule/inspection may be persisted. Successful key binding
uses a normal SessionAppend receipt with the exact typed argument hash and an
updated revision/time of the same child header. Replay after reopen creates no
new task/job or receipt and rechecks live state; changed arguments under the
same key conflict. A fresh handle is reconstructed and must be inspected again.
The existing input-bound compiler can refuse changed output parameters even
under another key; a new local task must be a separate owner choice.

`external_send` remains a separate explicit action with the exact callId/hash,
current policy and owner confirmation. Handles expire, disappear on lock or
reconfiguration, and never authorize an automatic resend. The existing job
quotas, invocation admission and response publication remain authoritative.
Preparation is a local write, not read-only status. Its head changes must not
cause Runtime to lose the original request/key or misattribute a late reply.
Use stable request ownership, withhold old body/handles on identity changes,
and follow existing single-send receipt recovery. Runtime must update its pin,
write catalog, request/reply handling and UI independently.

## Verification

Synthetic tests cover original child identity and no duplicated jobs, repeated
key/no-head-change, reopen replay and conflicting batch reuse, absent selections,
model mismatch, pause, source-read revocation, Vault lock, overlay refusal, exact
body confirmation and refusal to reprepare a completed invocation. No-port fakes
panic on credential/transport access during preparation. A separate counted
fake exchange runs only after explicit correct-hash confirmation; it is not HTTP
or real account evidence. Contract fixtures reject invalid token ceilings and
forged run/model/actor/path fields.

Required checks passed on Rust 1.98.1 GNU with offline Cargo. The workspace
test run retains mainline's `RUST_TEST_THREADS=4`; incremental compilation was
disabled to avoid filesystem hard-link cache warnings, without changing tests
or their deadlines.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked` | 414 passed, zero failures, including all three new batch-preparation scenarios |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| Independent Python 3.12.10 / python-jsonschema 4.26.0 cross-check | Passed: 37 schemas, 56 valid records, 126 record cases, 5,496 set records, 51 IPC messages, 120 Workspace messages, 46 store documents, 8 Provider invocation and 19 extraction-job cases |
| Runtime surface | Recorded/logged `9b25885480cf2e220b37f231fe9973afaecb3bd68b8939fdf1c918d42ac233b2`; 53 commands |
| Diff and public-content review | Passed; no added private files, credential-shaped strings or personal absolute paths |

Logs are retained in ignored `.local/history-batch-workspace.log` and
`.local/history-batch-clippy.log`. These checks do not diagnose or claim a fix
for the older default-parallel scheduling failures. All Provider activity in the
new tests is synthetic; the counted exchange is an in-process fake.

## Remaining work

Runtime must adopt the new preparation/inspection flow and preserve its existing
explicit send boundary. Batch response application/review and temporal/semantic
features still require their own product slices. This work provides no real
history/model quality, personal initialization, owner acceptance, independent
backup recovery, MV-8, MCP or cloud activation.
