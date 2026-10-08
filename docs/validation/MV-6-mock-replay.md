# MV-6 Core — Mock dispatch and reply replay

Date: 2026-10-06. Base: `4657f53`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, the existing local Mock session path and Runtime handoff. No provider, network, format, dependency or stage change.

## Reproduction

Three deterministic page-channel tests start from a synthetic saved input and capsule, representing the existing boundary before dispatch. The test ID source pauses each caller either at dispatch allocation or at the following commit allocation. The test harness alone retries the existing transient index-contention error until each caller reaches the selected boundary; this adds no production retry policy.

The old Core failed both positive replay cases: when the first call completed before the second resumed, the second receipt lookup succeeded but its earlier catalog pin could not read the newly published dispatch (`not_found`). When both callers reached dispatch commit after their initial lookup, the commit-time replay was ignored and a discarded dispatch ID entered the reply body, causing `idempotency_conflict`.

## Change

Both replay paths restore the authoritative dispatch ID and request hash from the published receipt. The receipt read uses the current catalog and its original revision, preserving historical-read barriers. Commit-time replay is resolved before the reply is saved. The restore checks capsule/request binding and rechecks the capsule's policy/deletion watermarks; a superseded authorization basis is refused.

This preserves the store's writer protocol, existing receipt/payload hashes and the separate dispatch/reply commits. It does not turn that pair into an atomic transaction or introduce real provider dispatch. Runtime adoption is recorded in the [handoff log](../integration/RUNTIME.md).

## Evidence

- `session_ask_replay_reads_a_receipt_published_after_its_initial_pin`: one caller completes before the other resumes at the earlier allocation boundary.
- `concurrent_session_ask_replay_uses_the_published_dispatch_and_reply`: both callers pass initial lookup and meet at commit allocation.
- `session_ask_replay_refuses_evidence_deleted_while_the_call_is_paused`: after the first answer is published, the owner logically deletes a synthetic cited memory before the second caller resumes. The second response is refused with no result.

The two positive tests validate full workspace envelopes, identical successful results, an inspectable/verified dispatch, one completed turn, exactly two transcript events, and a saved reply naming the actual dispatch. A subsequent retry with a fresh transport request ID returns the same result without moving the head, using Core's existing key-derived logical request ID.

A separate negative control temporarily removed only the new replay freshness check while keeping receipt restoration fixed. The deletion test then failed because both callers succeeded. The exact fixed source bytes were restored in `finally` and hash equality confirmed. The final focused tests all passed.

Final checks passed from the repository root with the pinned toolchain: `cargo fmt --all -- --check`, all 245 workspace Rust tests (Core 38), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The independent Python 3.12 / jsonschema 4.26.0 cross-check passed all 34 schemas. The integration checker confirmed the regenerated, logged surface aggregate `938a7fe63443c46b51578dee147df8aa591fcf0331c5a25a6a3cc0ddd5661449`; `git diff --check` passed. All fixtures are synthetic.

## Limits

These are synthetic full-page calls around already saved input/capsule state. They do not establish every concurrent execution starting before input creation, all deletion interleavings, different-key contention, transport disconnects, real Runtime adoption, accessibility, installation, signing, real-data recovery, OS restart or power-loss durability. The Mock remains offline. MV-7 is unopened.
