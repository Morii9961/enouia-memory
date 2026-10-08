# MV-6 Core — full session ask replay from initial input

Date: 2026-10-06. Base: `b4f8e72`. End: the acceptance commit containing this report. Scope: MV-6 / ADR-MEM-46, synthetic page-channel evidence for the existing local session pipeline. No production source, wire, schema, dependency or storage-format change.

## Covered boundary

The earlier session-service report tested input replay through the Context API. The Mock report tested full page requests starting from an already saved input and capsule. This slice starts with a synthetic session and canonical memory, with neither an input event nor a capsule prepared for the ask.

Two page callers use the same key and arguments but different transport request IDs. The shared synthetic ID source pauses them either at input event allocation or at input commit allocation. Existing retryable index contention and the exact `fault.writer_busy` rule are retried by the bounded test helper only. Other faults are returned immediately; no production retry policy is added.

## Evidence

- `full_session_ask_replays_an_input_published_while_the_call_is_paused`: both initial receipt lookups miss, then one whole call finishes before the other resumes its input allocation. The second call resolves the published input and continues through saved compilation, dispatch and reply replay.
- `full_session_ask_replays_a_concurrent_input_commit`: both callers meet at input commit allocation and are released together. Eventual page results agree, including when supported contention requires retry. This is not a claim that every first attempt reaches commit-time replay rather than a retry lookup.

Both cases check schema-valid successful envelopes, distinct transport IDs, identical returned saved IDs/results, the included synthetic memory, an inspectable verified dispatch, exactly one completed turn and two transcript events, and a saved reply naming that dispatch. A subsequent fresh transport request ID returns the same result without advancing the head. Changing the text under the same key returns `idempotency_conflict` with no result or head movement.

A negative control removed only the existing Context input's commit-time published-ID restoration. The ordered whole-call test failed with `not_found` for the discarded input ID; the concurrently released case passed in that control run, which is compatible with its allowed contention retry. Exact fixed production bytes were restored in `finally` and equal hashes confirmed. Both fixed full-page tests passed. The earlier service regression remains the separate direct proof for its commit-time return boundary.

Final pinned-toolchain root checks passed: `cargo fmt --all -- --check`, all 250 workspace Rust tests (Core 43), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Python 3.12 / jsonschema 4.26.0 passed all 34 schemas independently. The integration checker confirmed regenerated, logged aggregate `46c4446f34d0b789eb253fceb474bc86246cf7fc45f003a6028b45fd37ae9e38`; `git diff --check` passed. All evidence is synthetic.

## Limits

This adds two controlled same-key schedules starting before input creation, not proof of every concurrent session execution. Different-key contention, cancellation/transport disconnects, all policy/deletion interleavings, actual Runtime adoption, native Windows responsiveness, accessibility, signing, installation, real-data recovery and OS/power-loss durability remain unverified. The Mock is offline and MV-7 remains unopened. Runtime should keep host sequencing, retry only permitted identical requests and refresh state on other failures.
