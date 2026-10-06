# MV-6 Core — correction and forgetting request binding

Date: 2026-10-06. Base: `213e284`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, published Core correction and delete-proposal receipts. No stage, wire, schema, dependency or storage-format change.

## Reproduction and change

Governance's existing proposal replay lookup uses the owner/key scope. A Core correction retry with the same key and text but another memory returned the original candidate successfully. A forgetting retry could similarly switch the target, logical-delete/purge mode or dependent scope while receiving a plan for the original candidate. In particular, the old host-facing `purge` flag could describe the new request while the tombstone still described the old proposal.

Core now checks the original candidate revision named by a published receipt before and after proposal admission. Corrections bind target memory, expected revision and exact text. Forgetting binds target memory, mode and dependent scope. Mismatches return existing `idempotency_conflict` / `workspace.key_reuse` without a result. The shared receipt reader also preserves the earlier remember text/claim guard. Historical reads retain Vault purge barriers.

Forget requests carry no revision argument, so the guard does not bind a newly derived revision to the key. An identical retry still generates a fresh review plan using the current proposal/review preconditions. Correction's existing stale-target check remains: an identical old-revision request is refused after that memory changes. These guards do not bypass review, make source/proposal creation atomic or persist new request metadata.

## Evidence

- `correction_replay_binds_target_revision_and_text`: identical pending retries before and after Core reopen return the same candidate; changed target or text conflicts. An independent accepted update advances the target to revision two, after which reuse with the new revision conflicts and the original stale request still reports `revision_conflict`. Rejected sequential retries leave the head unchanged.
- `forget_replay_binds_target_mode_and_dependent_scope`: identical pending retries before and after Core reopen produce a plan whose tombstone still names the original target, mode and scope; changing any of these arguments conflicts with no result or head movement. Generated review/plan IDs are intentionally fresh.
- `correction_replay_rechecks_a_receipt_published_after_admission`: two full page requests use one key and text with different targets. A synthetic ID port pauses both before source publication, then releases one to finish. The later caller restores the source, encounters the newly published proposal receipt and is refused. Exactly one succeeds and one pending correction exists.

The three tests failed on the previous Core and passed after the correction. A separate negative control removed only the correction postcheck: the concurrent test failed because both callers succeeded. Fixed source bytes were restored in `finally`, with equal SHA-256 hashes confirmed. Fixtures, IDs, inputs and Vault roots are synthetic; reopen is in-process.

The first full suite also exposed a supported transient `busy` / `fault.writer_busy` response in the earlier Mock concurrency test. Vault's OS writer lock has a bounded wait; successful eventual replay must not be inferred from one contended attempt. The test helper now retries only retryable index contention or that exact writer-lock rule, with a bounded attempt count. It does not retry other faults or alter production behavior. All eleven focused replay tests passed with the corrected harness; the full suite was rerun on this final tree.

That rerun hit the earlier lifecycle test's 500 ms status-observation timeout; an isolated run passed. Status includes filesystem and ACL inspection, so this threshold did not isolate the intended lock-bypass property under parallel load. The test now waits at most five seconds, below its injected clock's ten-second pause deadline, and still requires the status result before releasing the held call. Removing only the lifecycle guard made this adjusted test fail immediately (`lock passed an in-flight Vault read`); exact fixed source bytes were restored. No product timing or Windows responsiveness claim follows from this test adjustment.

Final root checks on the pinned toolchain passed: `cargo fmt --all -- --check`, all 248 workspace Rust tests (Core 41), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Python 3.12 / jsonschema 4.26.0 passed the independent 34-schema cross-check. The integration checker confirmed the regenerated, logged aggregate `64fb36150372680ffbb06a20d58d55d946fee2f6023b53d9fd5199d7ce54956e`; `git diff --check` passed.

## Limits

Binding here requires a published proposal receipt. Pending duplicate detection may return a candidate without publishing a receipt for the new key; a source-only partial write does not bind the remaining logical proposal arguments. General governance proposal/edit/withdraw binding, every concurrency interleaving, Runtime adoption, real-data recovery and OS/power-loss durability remain outside this proof. Runtime must retain keys for identical input and assign fresh keys when arguments change. MV-7 remains unopened.
