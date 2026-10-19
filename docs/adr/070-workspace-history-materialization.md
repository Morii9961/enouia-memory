# ADR-MEM-70 — Local history materialization through Workspace

Date: 2026-10-19 (Asia/Shanghai).
Status: Owner-authorized MV-7 local integration; implementation in progress. No real history, transport, credential, MV-8 or cloud activation.

The owner approved explicit selection of archived batches followed by local pending-task creation and recovery. ADR-MEM-69's read-only plan remains separate. Runtime must use Workspace and exact revisions; it must not call private helpers or parse Vault data.

## Durable admission before child creation

Creating a coordinator Session and only later binding its input can leave a stub after a lost response. Workspace first needs an atomic, typed request admission: the first Session commit contains its exact import/revision/Raw binding, observed preview hash/head, bounds, ordered selected indices, verified text-model binding, derived owner subject, local quotas and submission-key hash. This is `SessionRecord.history_materialization`, optional for existing Sessions. It is canonical local task metadata, not an extension, grant, invocation or approved memory. It contains no source text, path, endpoint or credential.

The field is immutable across Session revisions. Per-record checks enforce closed shapes and bounded selections/models/quotas; whole/scoped validation requires the archived completed original import, matching Raw hash, known preview commit and owner subject. `session::start_history_materialization` binds the typed field into the ordinary SessionAppend payload and its first commit. Same-key replays retain the original Session; changing selection/options conflicts. Existing ordinary Session start payloads and missing-field serialization remain unchanged.

This first supporting API is not a page-facing write command. Workspace will independently validate current source readability/deletion/freshness and original preview binding before admission. A saved admission permits identifying the original request after reopen; it never permits model sending or bypasses later source checks. Any records created by the existing offline H2 helper remain ordinary source-linked Sessions/ExtractionJobs, with their normal governance.

## Following integration work

Workspace must expose bounded local task creation, status/discovery and explicit resume through typed IPC, with main-only host identity, exact original arguments/idempotency and no implicit retry. Status comes from durable admission/child records, never from a page-supplied progress cursor. A partial task stays distinguishable from completed materialization. Admission-only recovery must not create a second coordinator.

Runtime then pins the completed Memory revision, provides explicit bounded batch selection and local-write submission/recovery, and displays saved task/child identities. Neither creation nor progress reads expose a Provider/credential port. Sending, extraction application, review, temporal/semantic features and every cloud/Host stage remain separate actions or integrations.

## Gates

The Session schema, Rust validation, scoped/whole equivalence fixtures and atomic/reopen/conflict tests must agree; required Rust/Clippy/fmt, independent pinned schema checks and Runtime surface logging must pass before publishing a usable integration. The existing default-parallel Memory suite timing limits must be retained in reports. A Runtime pin/UI adoption requires its consumer, frontend and native checks. Current owner approval covers implementation and synthetic verification, not personal-data or real-model acceptance.
