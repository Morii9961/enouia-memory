# MV-6 Core — honest operation outcomes

Date: 2026-10-05. Base: `a1e0678`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46; synthetic operations, no Provider or Runtime code changes.

## Reproduced problem

Three regressions failed on the base: a requested cancel changed a subsequent `storage_failed` into task state `cancelled`; a worker's explicit cancellation error without a page request became `failed`; and cancelling an already successful task changed `cancelRequested` from false to true. The worker outcome match used the request flag instead of its error code, and cancellation unconditionally changed the flag without coordinating with terminal publication.

## Change

A worker cancellation error (`MemoryErrorCode::Cancelled`) or an explicit successful cancelled result reports `cancelled`. Other errors report `failed`, preserving their code/retryability; successful work reports `succeeded` even if a cancellation request arrived. Panics remain non-retryable failures. A cancel request is an intent, not proof of which result occurred.

Cancel admission now takes the task outcome lock and changes the request flag only for queued/running tasks. Terminal publication uses the same lock. Late cancel requests return the existing terminal description unchanged. Close also requests cancellation only from queued/running tasks, then joins workers and clears their history as before. No cancellation capability is added to verification or backup operations.

## Evidence

- `a_cancel_request_does_not_hide_a_worker_failure`: channel-controlled cancellation followed by storage, permission and busy errors; all remain failures with their original codes.
- `cancelling_a_terminal_operation_does_not_change_its_outcome`: succeeded, failed and cancelled tasks retain their exact descriptions after late cancellation.
- `a_worker_cancellation_is_reported_as_cancelled`: explicit worker cancellation without a page request.
- `completed_work_stays_successful_after_a_cancel_request`: accepted request followed by completed work still reports success and its saved result.
- Existing import/rebuild cancellation and panic-after-cancel tests continue to pass.

Required repository-root checks passed with pinned Rust 1.98.1 GNU offline: format, **226 workspace tests** (including **24 Core tests**), Clippy with warnings denied, independent Python 3.12 / python-jsonschema 4.26.0 verification of **34 schemas**, and recorded/logged Runtime surface aggregate `8f6dbe664c7cb4333b5f775a150c78b806e5da95d6a2280502b7b4521ea4a9cd`. The Rust suite ran outside the sandbox for the existing synthetic Windows ACL tests. `git diff --check` passed; logs remain ignored.

## Limits

These tests use local synthetic workers and existing Core command envelopes. They do not establish Runtime task messaging, actual Windows UI/installer behavior, real data, Provider cancellation, signing, OS crashes or power-loss durability. Cancellation remains cooperative; `cancelRequested` does not imply that work stopped or that a completed write was undone.
