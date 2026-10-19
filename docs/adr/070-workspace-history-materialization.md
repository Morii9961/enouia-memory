# ADR-MEM-70 — Local history materialization through Workspace

Date: 2026-10-19 (Asia/Shanghai).
Status: Owner-authorized MV-7 local Core integration implemented and synthetically verified; Runtime adoption pending. No real history, transport, credential, MV-8 or cloud activation.

The owner approved explicit selection of archived batches followed by local pending-task creation and recovery. ADR-MEM-69's read-only plan remains separate. Runtime must use Workspace and exact revisions; it must not call private helpers or parse Vault data.

## Durable admission before child creation

Creating a coordinator Session and only later binding its input can leave a stub after a lost response. Workspace first needs an atomic, typed request admission: the first Session commit contains its exact import/revision/Raw binding, observed preview hash/head, bounds, ordered selected indices, exact configured model binding, derived owner subject, local quotas and submission-key hash. This is `SessionRecord.history_materialization`, optional for existing Sessions. It is canonical local task metadata, not an extension, grant, invocation or approved memory. It contains no source text, path, endpoint or credential.

The field is immutable across Session revisions. Per-record checks enforce closed shapes and bounded selections/models/quotas; whole/scoped validation requires the archived completed original import, matching Raw hash, known preview commit and owner subject. `session::start_history_materialization` binds the typed field into the ordinary SessionAppend payload and its first commit. Same-key replays retain the original Session; changing selection/options conflicts. Existing ordinary Session start payloads and missing-field serialization remain unchanged.

The supporting admission API alone is not a page-facing write command. Workspace independently validates current source readability/deletion/freshness and original preview binding before admission. A saved admission permits identifying the original request after reopen; it never permits model sending or bypasses later source checks. Any records created by the existing offline H2 helper remain ordinary source-linked Sessions/ExtractionJobs, with their normal governance.

## Workspace page contract

The implementation adds four commands to IPC v1 (52 total), verified with synthetic Vaults:

| Command | Arguments and result | Writes |
| --- | --- | --- |
| `history_materialize` | Exact import/revision/Raw, original plan hash/head and planning bounds, 1–32 unique ordered batch indices, API/exact configured model, per-child candidate/pending/token/nullable-cost bounds. Returns an ordinary operation ID. | Yes; keyed, long-running. |
| `history_task_list` | Standard bounded cursor/limit; returns task IDs, creation times and saved request descriptors bound to the observed Vault/head. | No. |
| `history_task_get` | Task Session ID; returns verified coordinator/child IDs, actual job/invocation states and missing selected indices. | No. |
| `history_resume` | Task Session ID; returns an operation ID and creates only missing ordinary records under the original task-derived keys. | Yes; keyed, long-running. |

Only a new admission requires the original current/opening-bound H1 preview and exact native model configuration. Reopen or same-key recovery compares the complete saved request, retains the original admission and rechecks current import/source bindings without demanding the old head: its own writes changed that head. Owner subject and genesis access policy are native-derived. Page arguments cannot assert actor, subject, policy, paths, endpoints, source text or progress.

The admission Session ID is the durable task ID. Ordinary helper keys derive from it; read-only Provider inspection reconstructs coordinator and child state from canonical receipts/records while revalidating selected source bindings. A coordinator without input or an incomplete child remains explicitly missing. No read repairs it. `materializationComplete` means all selected local jobs exist, independently of extraction completion, invocation outcome or memory review. The caller must preserve its original request/key on an ambiguous response, discover saved tasks and use explicit resume rather than automatically submitting a fresh task. Status reads and resumes still refuse revoked/deleted/stale sources.

## Following integration work

Workspace verification must cover bounded creation, status/discovery and explicit resume through typed IPC, main-only host identity, exact original arguments/idempotency and no implicit retry. Status comes from durable admission/child records, never from a page-supplied progress cursor. A partial task stays distinguishable from completed materialization. Admission-only recovery must not create a second coordinator.

Runtime then pins the completed Memory revision, provides explicit bounded batch selection and local-write submission/recovery, and displays saved task/child identities. Neither creation nor progress reads expose a Provider/credential port. Sending, extraction application, review, temporal/semantic features and every cloud/Host stage remain separate actions or integrations.

## Gates

The Session schema, Rust validation, scoped/whole equivalence fixtures and atomic/reopen/conflict tests must agree; required Rust/Clippy/fmt, independent pinned schema checks and Runtime surface logging must pass before publishing a usable integration. The existing default-parallel Memory suite timing limits must be retained in reports. A Runtime pin/UI adoption requires its consumer, frontend and native checks. Current owner approval covers implementation and synthetic verification, not personal-data or real-model acceptance.
