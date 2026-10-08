# MV-7 native text Provider implementation

Date: 2026-10-08. Baseline: `8197890`. Scope: Memory repository only.

The owner explicitly started MV-7 and then instructed: implement functionality first, defer real smoke. This report records native Rust functionality and synthetic acceptance. It does not mark the MV-7 real-account exit gate passed. No real API credential was read/written, no real account was accessed and no HTTP call was made during validation.

## Implemented

- OpenAI Responses and Anthropic Messages text requests, canonical exact-body inspection, model/destination binding and strict JSON/SSE decoding. OpenAI disables storage/automatic truncation; native HTTPS disables redirects, implicit proxies and automatic retries, with bounded input/response/timeout/cancellation.
- Saved external Context compilation retains approved evidence and source references, applies destination-specific standing policies, and keeps local Mock serialization/behavior. Native owner policy grant/revocation plans are hash-confirmed. Governance `AcceptWithEgress` binds explicit egress metadata through the existing reviewed plan; ordinary approval does not enable external use.
- Private content needs an exact, expiring, single-use per-request approval. Highly sensitive content is refused. Current policy/deletion/lock/restore gates and durable audit precede HTTP admission. Missing credentials, capabilities, approval, quota or audit fail without an attempt.
- Admission atomically persists immutable Dispatch, message bodies, full wire body and a Session invocation reservation. Same input/dispatch cannot be resent, even with an unknown/crashed outcome or a new dispatch ID. Known usage, terminal errors, local token/price reservations and incomplete length results are retained.
- Stream chunks are saved before delivery. Responses inherit the strictest carried evidence sensitivity. Publishing private text checks original policy/deletion epochs against the writer's pinned head; final delivery checks authorization again. EOF is never completion.
- Exact archived inspection and saved results survive reopen. A terminal-receipt/ledger publication gap is repaired locally without HTTP. Existing dependent deletion now covers full request archives and length-failure response bodies; the shared-evidence purge leaves a clean Vault.
- Native Windows credential read/write targets are restricted to the two named Provider entries. Those functions are compiled, not exercised against the owner's credential store.
- Optional source-bound extraction persists immutable selection/prompt/binding/budgets and progress. Whole-response JSON/quotation validation precedes proposals; candidates remain pending, with source-derived evidence class/sensitivity. Pause, local cursor receipt recovery, unchanged reviewed-claim suppression and bounded pending backlog are enforced. Source-dependent deletion erases undispatched input too. Immutable job token/monetary caps survive reopening and constrain the caller’s Session quota; required prices cannot be omitted. Input safety margin is reserved for tokens and cost.

## Synthetic acceptance

`crates/enouia-memory-provider/tests/providers.rs` uses isolated Vaults, synthetic evidence, fake secrets and fake byte transports. Its original 13 cases cover both API envelopes, exact archives/replay, private approval and changed wire refusal, missing keys/capabilities/lock/quotas, admitted crash/new-dispatch refusal, fragmented UTF-8 SSE, both streaming terminal/truncated cases, network failure, reopen/local receipt recovery, explicit price ceilings, shared reviewed evidence and purge, standing-policy revocation, length-preserving incomplete outcomes and cancellation. The shared-evidence case also proves that private evidence yields a private result in an otherwise normal Session.

Seven additional extraction cases cover both APIs, source-only preparation, no canonical writes, enable/pause/resume, invalid later claims leaving zero candidates, unchanged rejection suppression, changed-key/UTF-8/backlog refusal, reopen after injected publication failure, deletion before dispatch and durable token/price/pause admission limits. Seventeen extraction-job fixtures distinguish schema shape from typed cross-field constraints. Storage validation also binds source content hashes and candidate IDs; added property cases prove scoped/whole-set equivalence for extraction jobs.

Contract/schema acceptance adds eight positive/negative invocation cases, checked independently by serde/domain validation, the Rust subset validator and pinned python-jsonschema. Existing fixtures remain synthetic; no Runtime checkout, fixtures or target directory is used.

| Check | Result |
|---|---|
| Pinned Rust formatting | Passed: `cargo fmt --all -- --check` |
| Offline `cargo test --workspace --locked` | Passed: 295 tests, including 20 Provider cases and 60 workspace Core cases. Recovery/deletion and bounded object reads are included in this final run. |
| Offline strict workspace Clippy, all targets | Passed: `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Python 3.12 / python-jsonschema 4.26.0 | Passed: 36 schemas; 53 valid records; 117 record mutations; 4,913 set records; 51 IPC, 63 workspace, 46 store, 8 invocation and 17 extraction-job cases |
| Runtime integration manifest/log | Passed: aggregate `f1a52c732445fe9ea0c5d5b33eaec3fd5fe7b9a71376ca3215e594f1792cf167` |

The initial full check detected the missing actual aggregate in the compatibility row; that row was corrected and the full check repeated. Earlier targeted checks exposed test-only shared idempotency/claim labels and an incorrect source-delete scope; those fixtures were corrected. Extraction preparation also exposed an inappropriate normal-search query budget; extraction now compiles selected sources directly. These failures did not cause live calls.

## Host adoption and acceptance still pending

[ADR-MEM-47](../adr/047-explicit-provider-dispatch.md) and the [native guide](../integration/PROVIDERS.md) document the API flow and failure semantics. [Runtime's compatibility row](../integration/RUNTIME.md) records adoption duties. `SessionRecord.provider_invocations` and `extraction_jobs` are omitted when empty; existing Mock sessions retain their previous serialization. Populated ledgers/jobs are rejected by older strict Session readers, so the Runtime pin must be updated before enabling Provider-generated sessions.

Workspace IPC/reference UI still call Mock; Runtime product Provider controls, native credential setup, worker cancellation/disable/join and rendered/native acceptance are not implemented in this checkout. Core does not depend on the HTTP crate. Real account/model capabilities, pricing, exact tokenizer integration, actual HTTP transport/SSE behavior and live cancellation remain unverified. Subscription ownership is not treated as proof of API access.

This slice estimates token input from full UTF-8 wire bytes plus a margin; it is not an exact tokenizer guarantee. Reservations/price caps apply per Session, with explicit caller-supplied rates, not global account spend. Unknown admissions never auto-refund or resend. Recovery of a terminal length result before ledger publication may conservatively retain `failed`; usage can remain unknown after that recovery. Optional model extraction (MV-7.4) is implemented as an explicit native flow and remains inactive in the host. Its candidates enter the existing owner review queue. Only fact/preference claims are supported; literal quotations are verified for containment, not semantic truth. Raw objects over 64 MiB are refused before allocation. Precise snippet offsets remain in the job/prompt/response; candidate evidence retains whole-source locators. MV-8 has not started.
