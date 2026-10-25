# Constraint enforcement map (MV-0R)

For each constraint family, this map shows which layer enforces it:

- **Schema**: JSON Schemas under `contracts/`, checked two ways. The in-repo subset validator runs in `cargo test`, and the pinned python-jsonschema 4.26.0 runs in `tools/schema-check`. Both must agree on every fixture.
- **Rust record**: `parse_value` plus each record's `validate()`.
- **Rust set**: `set::validate_set`.
- **Later behavior**: tests that need a real store or resource.

**Passing Schema and Rust validation does not prove that a transaction is atomic, durable, or recoverable.**

MV-1 update: rows whose "Later behavior" names MV-1 now have store evidence (commit protocol, crash matrix, D02 migration, CSPRNG, clock regression, digest recomputation on read); see the [MV-1 report](../validation/MV-1.md) §2. Store file contracts are listed separately in `tests/fixtures/store/store-manifest.json` (10 documents, 36 invalid cases; MV-3.0 added catalog segments and stored commits, ADR-MEM-39).

| Constraint family | Schema | Rust record | Rust set | Later behavior |
|---|---|---|---|---|
| Required fields; explicit null | ✔ | ✔ (missing key is an error) | – | – |
| Unknown fields rejected; namespaced extensions preserved | ✔ | ✔ | – | MV-1 migration keeps extensions |
| Enums (no unknown sensitivity, status or scope) | ✔ | ✔ | – | – |
| ID namespaces | ✔ | ✔ typed IDs (a review ID cannot fill an approval field) | – | MV-1 CSPRNG |
| UTC timestamps; real dates | pattern only | ✔ real dates | commit time order | MV-1 clock |
| Integer range ±(2^53−1); checked sums (F6) | ✔ | ✔ `number.out_of_range`, `capsule.budget` | bounded checkpoint scan | release build tested |
| Major version read-only | ✔ | ✔ | – | **MV-1** failed migration keeps old `CURRENT` (D02) |
| Source role vs evidence class | partly | ✔ | ✔ `evidence.class_mismatch` | – |
| Evidence closure | ✗ | primary source and item evidence | ✔ `evidence.*` | MV-1 recomputes object digests |
| Sensitivity never downgraded without an exact declassification approval (F2) | ✗ | approval direction and sources | ✔ `sensitivity.downgrade`, `sensitivity.declassification_invalid` | – |
| Secrets and local paths out of text | ✗ | ✔ heuristic | – | MV-2 quarantine; B04 log scan |
| Candidate lifecycle | ✔ | ✔ | ✔ | MV-3 engine |
| Canonical write needs an owner review of the exact pending revision | ✔ | ✔ | ✔ review/nonce/commit binding | **MV-3** nonce issuance and expiry, batch all-or-nothing |
| Deletion is owner-confirmed and bound (F2) | ✔ `delete_binding` required | ✔ `review.delete_binding` | ✔ `tombstone.review_action/binding/result/candidate_mismatch` | **MV-3** purge propagation |
| Supersession graph | ✗ | self/duplicate edge | ✔ `supersession.*`, including `effective_mismatch` | – |
| Unknown supersession start stays unknown (F5) | ✗ | – | ✔ `temporal::effect`/`currency` | MV-4 queries |
| Conflicts shown on both sides | ✗ | flag | ✔ | MV-5 |
| Checkpoint provisional vs reviewed | ✔ | ✔ | ✔ | MV-5 recovery |
| Session events | ✔ | ✔ | ✔ | MV-5 streaming |
| Commit manifest shape; chain, epochs, catalog | ✔ | ✔ | ✔ | **MV-1** atomic `CURRENT`, crash, power loss |
| Idempotency | ✗ | `classify_retry` | ✔ | **MV-1** lost response → same receipt |
| Policies: persistent, versioned, revocable, default deny (F4) | ✔ | ✔ `policy.*` | ✔ `policy.unresolved`, `policy.grant_invalid` | MV-6/7 real gate wiring |
| Project- and Provider-scoped authorization (F4) | ✗ | `policy::evaluate` | ✔ capsule and dispatch use it | MV-7 real Provider |
| Egress: highly sensitive never; external needs a grant; private needs an exact approval (F1) | ✗ | approval TTL, owner, destination | ✔ `dispatch.policy_*`, `egress.approval_*` | MV-7 |
| Dispatch covers every carried resource (F1) | ✔ `resource_refs` | ✔ `dispatch.resource_kind` | ✔ `dispatch.hidden_resource` | MV-5 real render |
| Dispatch digest bound to the actual request (F3) | ✗ | ✔ `dispatch.request_hash`; `ProviderRequest::verify_against` | ✔ approval payload binding | MV-5/7 send path |
| Deletion/policy barrier rechecked before send | ✗ | – | ✔ | MV-5/7 live recheck |
| IPC request/response contracts | ✔ | ✔ `ipc.*` | – | transports: reference shell / Runtime host (MV-6), Host/MCP (MV-8) |
| Import closure: sources cite an existing import and its bytes; completed coverage counts its sources; revisions keep the received bytes (`import.input_changed`) | received hash format | ✔ ImportManifest rules | ✔ `check_imports` | MV-2 reconciliation re-derives every cited source |
| Commit validated on a scoped set equals whole-Vault validation (ADR-MEM-39) | – | – | ✔ `delta::validate_delta`, property test `tests/delta.rs` | MV-3.0 store uses it for every commit |
| Self-contained build; no Runtime/Activity dependency | – | `tests/boundaries.rs` | – | **MV-1** installed-artifact run (D03/D04 behavior) |
| Embedded Core stays transport-neutral: workspace Core depends only on this repository; no domain crate depends on a window toolkit (ADR-MEM-45) | – | `crates/enouia-memory-contract/tests/boundaries.rs` `memory_workspace_depends_only_on_this_repository`, `no_domain_crate_depends_on_a_window_toolkit` | – | – |
| Host caller scope: the native window identity selects a `HostSurface`; quick search reaches `memory_search` only; packet fields never widen it (ADR-MEM-45) | – | `HostSurface::allows`; `crates/enouia-memory-contract/tests/workspace.rs` `host_surfaces_scope_requests_by_native_identity_only` | – | reference shell `window_scope` tests (`apps/workspace/src-tauri/src/window_scope.rs`); the Runtime host must apply the same rule |
| One embedded Core per Vault; root rejection reasons; confirm and import retries; contained operation panics (ADR-MEM-46) | – | workspace Core tests in `crates/enouia-memory-workspace/src/tests.rs` | – | real-app evidence on the Runtime host |
| Runtime integration surface changes recorded and logged (ADR-MEM-45) | – | `crates/enouia-memory-contract/tests/runtime_surface.rs` | – | regenerated with `tools/integration/runtime_surface.py`; cannot prove that Runtime adopted a change |

## Rule coverage

Core-7 (ADR-MEM-91) adds closed page/detail/simulation DTOs with explicit native
capability/effect and unavailable product capabilities. Schema/native fixture
cases cover identity, receipt binding, unknown versions, false capability flags,
source hashes, duplicates and simulation byte counts. Native read-model cases
test scope before counts/edges, cursor snapshot/filter/profile/sensitivity binding,
selected history/source text/actual commit times, deletion/lock and no-write
simulation. Typed consistency checks additionally reject non-progressing page
cursors, duplicate/unordered or contradictory history, duplicate source revisions
and unbalanced/duplicate selected/excluded recall identities. Native tests cover
event-window boundaries/unknown dates, validity distinct from adoption/state,
partial byte-bounded output and clock rollback.
Parsing these DTOs is not authentication or a new write/approval authority.

Core-6 (ADR-MEM-90) adds closed attention profile/projection schemas and native
selection over an already authorized set. Pure tests cover aliases, fixed-clock
replay, conditions/time/staging, accessibility-only decay, historical rescue,
budgets and duplicate observations. Native cases additionally prove permission
before counts, current validity independent of ranking time, cache nonauthority,
source deletion/lock and single cancellable periodic maintenance. None changes
record truth, adoption or plan state, and no notification/Activity source exists.

Core-5 (ADR-MEM-89) adds `next/formation-v2.schema.json`, policy rule-3/layout
gates and source-bound semantic dimensions. Pure formation tests check states,
unknown time, source/parent closure, qualified understanding, exact reversion
and independent omission detection. Native formation cases exercise the same
transactions across restart, correction/pause/cancel, source-preserving repeated
synthesis, stale parents, expiry and Core-4 fake policy. The comparison is
typed representation, not extraction/relationship/emotion quality evidence.

Core-4 (ADR-MEM-88) adds `next/continuation-v2.schema.json`, 17 schema/native
fixture cases and three contract tests for scope, history and conservative
charges. Native automatic-loop continuation cases test policy absence/grant,
withdrawal before/after generation, lifetime budgets, expiry/lock, unknown and
overrun costs, write failures, corrupt versions, leases and child-process crash.
The shared policy barrier also gates Core-3 application. No V1 private-send
approval rule is relaxed; all transports in this slice are fake.

Core-3 (ADR-MEM-87) adds closed descriptor/current/commit/publication documents
in `next/storage-v2.schema.json`. The automation-vault fixture manifest checks
17 schema/native cases; `tests/automation_storage.rs` additionally checks full
set/delta, authority, exact batches and immutable history. `next_admission.rs`
checks rule-2 same-policy native creation provenance and legacy-owner refusal.
Native `host/tests/automatic_loop.rs` exercises real files/leases, short writer
exclusion, automatic completion, exact replay, current authorization, legacy
byte preservation and actual child-process termination at three publication
boundaries. Schema checks shape, typed rules check semantic closure, and native
tests check publication/recovery; none establishes model quality or real
Provider/Runtime acceptance. See [Core-3 evidence](../validation/CORE3-automatic-loop.md).

Core-2 (ADR-MEM-86) adds the isolated native `memory_job_journal` schema and
`host::automation::Journal` validator. The synthetic automation manifest compares
schema shape with cursor/state/key/reservation consistency. Host automation tests
exercise actual atomic files, lifetime OS exclusion, process termination,
current Core-1 admission, original-ID dedupe, foreground/cancellation gates,
budgets, deadlines, panics and unknown publication recovery. These are job-state
checks, not Canonical set validation or real Provider/Runtime acceptance.

Core-1 (ADR-MEM-85) adds isolated `next` document rules without changing v1
record/set validation. `tests/fixtures/next/manifest.json` compares schema shape
with native rules for versions, System identity, budgets, evidence, outcomes
and planned commit identity. `tests/next_admission.rs` in the contract crate
covers current policy/snapshot binding, dependency closure, source freshness,
access/sensitivity inheritance, exact evidence/subject attribution, duplicate
origins, target revisions and prospective commit association. These require
trusted context and cannot be established by JSON Schema alone. The independent
Python checker reads the same manifest. See
[the contract](CORE1_AUTOMATIC_ADMISSION.md) for limits: no authentication,
semantic-quality proof, production writer or successful-commit receipt.

MV-7 adds Provider invocation and optional extraction-job schemas. Invocation terminal/error shape is checked by both schema and typed validation. Extraction selection count, fixed version/binding and scalar budgets are schema rules; offset order/span and combined-byte limits, cursor/array agreement and cursor budget are typed rules. `extraction-job-cases.json` explicitly records these differences for both validators. Set validation binds the job to its saved input/source references, exact source content hashes, existing candidate IDs and immutable history, including paused/completed progress. Scoped validation explicitly loads the selected source revisions; synthetic tests compare it with whole-set validation. Native execution verifies actual source/snippet hashes, UTF-8 offsets, egress, exact quoted support, backlog and receipt recovery. These synthetic checks do not prove live model/account behavior.

The pre-MV-7 coverage inventory counted 265 stable rule identifiers, with 156 asserted by a named negative fixture or test. The new Provider/extraction families above are additional; that inventory has not yet been recounted. The rest are implemented, and several of them fire as side effects of other cases, but no fixture targets them individually yet; they are mostly field-consistency checks within the families above. Adding dedicated cases for them is cheap MV-1 hardening. `tests/fixtures/memory/*-manifest.json` lists every case with the rule it proves.

ADR-MEM-70 adds optional typed local history admission on Session headers. Both Schema and typed validation enforce a closed bounded selection/model/quota object; explicit null is rejected rather than silently rewritten to absence. Set rules bind the completed original import/Raw hash, preview commit and owner subject, and prohibit adding/changing/removing admission on later Session revisions. Delta needs include the referenced import revision and preview commit; record/set fixtures and scoped/whole equivalence cover these constraints. Atomic Session creation binds the admission to its submission key and current preview head; this is local request persistence, never egress approval or extraction completion.

## MV-8.1a authority contract

`host/authority-v1.schema.json` and `host/transfer-plan-v1.schema.json` constrain strict fields, typed IDs, safe counters, phase tags and the 128-receipt budget. `host::TransferPlan::validate` checks distinct target and canonical intent hash. `host::AuthorityState::validate` checks receipt chain, primary epoch/host, checkpoint and quiesced-plan binding. Transition methods reject stale heads, wrong writers, changed approvals, mismatched restored checkpoints, watermark rollback and exhausted counters. `tests/fixtures/host/manifest.json` names schema-versus-semantic negatives; `tests/host.rs` exercises transitions, complete receipt replay, prepublication failure, lost response, unavailable authority and stale expected state. Python's independent validator checks the same host fixtures. These are pure/synthetic contracts, not existing-store enforcement, native authentication, physical recovery or live migration evidence.
