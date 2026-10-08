# MV-6 Core — concurrent session replay IDs

Date: 2026-10-06. Base: `06a1775`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, the Context session services used by embedded Core and their Runtime handoff. No format, dependency, provider or stage change.

## Problem and reproduction

Context session operations check receipts before preparing a write. If two identical calls both pass that lookup, the store serializes publication and returns `Replayed` to one call. Session creation, event append, fork and checkpoint nevertheless returned their discarded newly allocated IDs.

Four deterministic regressions failed on the old implementation: an actual pair of `session_new` page calls returned different session/branch pairs despite only one session publication; direct Context input, fork and checkpoint calls each returned different event/branch/checkpoint IDs. The test ID source pauses both calls at their first allocation after initial receipt lookup, then releases them. All fixtures are synthetic and isolated.

## Change

Each operation checks a commit-time replay and obtains its returned ID from the published receipt. Session and fork results read the receipt's historical session revision to recover the original branch; later session changes do not change the replay result. A missing expected receipt record fails instead of returning an unpublished ID. Ordinary committed writes retain their normal fast path.

The existing payload hashes, writer serialization, optimistic preconditions and historical-read purge barriers remain in force. This does not make different request IDs identical, or combine save/compile/answer into one transaction. Runtime must adopt the deeper Context behavior with its pinned revision and continue sequencing UI requests. See the [handoff log](../integration/RUNTIME.md).

## Evidence

- `concurrent_session_new_replays_the_published_session_and_branch`: two schema-validated page responses agree; exactly one session exists, its detail is readable, and subsequent replay returns the same IDs.
- `concurrent_context_session_input_replays_the_published_id`: service results agree and name a published event; identical subsequent replay does not move the head.
- `concurrent_context_session_fork_replays_the_published_id`: results agree and name a persisted branch; a later separate fork does not alter the original replayed branch.
- `concurrent_context_session_checkpoint_replays_the_published_id`: results agree and name a published checkpoint; subsequent replay preserves the head.

The previous manual-assertion regression now shares the same test-only allocation harness and still passes. No allocation hook is added to product code. The six focused concurrency checks passed after correction, and the strengthened fork history check passed separately.

Final pinned-toolchain checks passed: `cargo fmt --all -- --check`, all **242 Rust tests** in `cargo test --workspace --locked` (35 Core tests), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The initial Clippy run found one redundant borrow in the test harness; that was removed, and the final complete suite and Clippy passed. Python 3.12 / python-jsonschema 4.26.0 passed the independent 34-schema cross-check. The Runtime checker passed with aggregate `f20c2eec11d022e3c7a6ccdf9efdaa012be674e51a35964f7b366ec2ef9fa292` logged; `git diff --check` passed.

## Limits

Only session creation is exercised as two full workspace page calls here; input/fork/checkpoint concurrency is exercised through their actual Context service APIs. These checks do not establish concurrent full `session_ask`, transport retries with a new request ID, different-key contention, other Vault convenience services, real Runtime/UI acceptance, OS restart, real-data recovery or durability. Runtime adoption, accessibility, signing and installation gates remain pending. MV-7 is unopened.
