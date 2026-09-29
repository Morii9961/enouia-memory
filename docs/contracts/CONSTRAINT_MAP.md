# Constraint enforcement map (MV-0R)

For each constraint family, this map shows which layer enforces it:

- **Schema**: JSON Schemas under `contracts/`, checked two ways. The in-repo subset validator runs in `cargo test`, and the pinned python-jsonschema 4.26.0 runs in `tools/schema-check`. Both must agree on every fixture.
- **Rust record**: `parse_value` plus each record's `validate()`.
- **Rust set**: `set::validate_set`.
- **Later behavior**: tests that need a real store or resource.

**Passing Schema and Rust validation does not prove that a transaction is atomic, durable, or recoverable.**

MV-1 update: rows whose "Later behavior" names MV-1 now have store evidence (commit protocol, crash matrix, D02 migration, CSPRNG, clock regression, digest recomputation on read); see the [MV-1 report](../validation/MV-1.md) §2. Store file contracts are listed separately in `tests/fixtures/store/store-manifest.json` (7 documents, 23 invalid cases).

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
| IPC request/response contracts | ✔ | ✔ `ipc.*` | – | MV-6/8 transports |
| Import closure: sources cite an existing import and its bytes; completed coverage counts its sources; revisions keep the received bytes (`import.input_changed`) | received hash format | ✔ ImportManifest rules | ✔ `check_imports` | MV-2 reconciliation re-derives every cited source |
| Commit validated on a scoped set equals whole-Vault validation (ADR-MEM-39) | – | – | ✔ `delta::validate_delta`, property test `tests/delta.rs` | MV-3.0 store uses it for every commit |
| Self-contained build; no Runtime/Activity dependency | – | `tests/boundaries.rs` | – | **MV-1** installed-artifact run (D03/D04 behavior) |

## Rule coverage

The validators define 265 stable rule identifiers. For 156 of them, a named negative fixture or test asserts that rule specifically. The rest are implemented, and several of them fire as side effects of other cases, but no fixture targets them individually yet; they are mostly field-consistency checks within the families above. Adding dedicated cases for them is cheap MV-1 hardening. `tests/fixtures/memory/*-manifest.json` lists every case with the rule it proves.
