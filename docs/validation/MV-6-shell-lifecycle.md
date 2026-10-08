# MV-6 reference shell — lifecycle scheduling

Date: 2026-10-05. Base: `be71bd9`. End: the feature commit containing this report. Scope: MV-6 reference shell / ADR-MEM-46, not a new product UI or Runtime change.

## Problem and negative control

The page exit command and tray lock/exit callbacks called blocking Core lifecycle methods inline. These methods wait for in-flight calls and join long operations. That could block the calling executor or native menu event thread until cleanup finished.

The new scheduling tests were run against equivalent inline helpers: each helper performed `call`/`shutdown` before returning its join handle. All three original tests failed with `native admission blocked` at roughly **10.02 seconds**, the synthetic worker's release timeout. Fixed helpers were restored from an exact local copy before final checks. This is a negative control of the scheduling boundary, not an observed Windows UI freeze.

## Change

The reference shell now shares `src-tauri/src/lifecycle.rs`: tray lock queues `call` on a blocking worker; tray exit queues `shutdown` followed by its process-exit callback; page exit asynchronously awaits a shutdown worker and then exits. A worker failure returns the opaque host `worker_failed` from page exit. Native callbacks return after queueing, while Core still waits/joins safely. No force-exit deadline or cancellation capability is added.

The module uses the same pinned Tauri 2.12.0 `spawn_blocking` pattern already used by page requests and native pickers. Core/wire signatures and dependencies remain unchanged. Runtime must adapt its own lifecycle scheduling through the [handoff log](../integration/RUNTIME.md).

## Evidence

Synthetic isolated Vaults and channel-controlled workers cover:

- `queued_lock_returns_before_join_and_finishes_locked`.
- `queued_shutdown_returns_before_join`.
- `exit_callback_runs_after_cleanup_on_a_worker`: callback sees empty operation history and runs on a different thread from admission.
- `dropping_a_tray_exit_handle_does_not_cancel_cleanup`.
- `dropping_a_tray_lock_handle_does_not_cancel_cleanup`.

Each admission returns while its Core worker is still pending, then cleanup completes only after the test releases that worker. The two detached-handle tests exercise the tray's fire-and-forget scheduling. No test hook or extra capability is compiled into the product command surface.

Final checks on the pinned toolchain passed: `cargo fmt --all -- --check`, all **231 Rust tests** in `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The independent Python 3.12 / python-jsonschema 4.26.0 cross-check passed all 34 schemas; the Runtime surface checker passed with aggregate `4a39ccdddc84a7b9a024edc085ce2b0c0f5e424524caa3e9edc143397a0d5f5a` recorded in the handoff log. `git diff --check` passed.

## Limits

These are local synthetic helper/command and build checks. Actual Windows menu/page responsiveness, installer behavior, Narrator, Runtime adoption, real-data recovery, Provider calls, signing, OS crashes and power-loss durability remain unverified. Cleanup can still wait on uncancellable disk operations; this change moves the wait off the event thread rather than guaranteeing a bounded exit time.
