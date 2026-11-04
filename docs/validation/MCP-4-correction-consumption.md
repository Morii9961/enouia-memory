# MCP-4 System consumption of saved correction reports

2026-11-03. Base `5f6c147`. [ADR122](../adr/122-system-correction-consumption.md)
and [consumption contract](../contracts/AGENT_CORRECTION_CONSUMPTION_V1.md).

The captured formation worker now consumes each saved correction report once
through a separate native CorrectionAnalyzer port. An accepted statement becomes
an ordinary target-bound `correction` revision decided by the existing evaluator
and `apply_batch`; no Owner queue or action is involved. The Host gains opt-in
`start_worker_with_corrections`; `start_worker` and CaptureAnalyzer v3 are unchanged.

Owned files: Vault `formation_worker.rs` and `mod.rs`; Host
`captured_worker.rs`; Host tests `agent_correction.rs` (3 consumer tests) and
`owner_runtime_capture.rs` (1 end-to-end worker test); ADR122, register,
consumption contract, this report, Runtime log, regenerated surface and the
continuation log. No schema or validator changed.

## Finding fixed during this slice

Native `prepare` rebinds the head itself, so its Stale for a target-bound seed
means the target moved or vanished. The worker classified Stale as retryable
and looped forever on such work (found by the moved-target test). Target-bound
work is now refused on that Stale. Captured seeds have no target and keep their
behavior.

## Checks on the final tree

Pinned Rust 1.98.1, offline/locked, serial Windows tests, owned synthetic roots
and an owned synthetic peer process. No real Vault, model, Provider, client
configuration or Runtime checkout.

- `cargo test -p enouia-memory-host --test agent_correction --locked`: **11 passed**.
- `cargo test -p enouia-memory-host --test owner_runtime_capture --locked`:
  **17 passed**; the new end-to-end test also passed 3/3 alone.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed; log
  `target/correction-consumer-workspace-clippy.log`. No Rust source changed after it.
- Full workspace, `--no-fail-fast`: **913 passed, 0 failed, 4 ignored** across
  110 distinct targets (909 before this slice plus 4 new tests). The background
  run was killed by a session pause after 98 targets; the remaining
  `enouia-memory-workspace`/`-app` packages and all doc-tests were then run on
  the unchanged tree. Logs: `target/correction-consumer-workspace-tests.log`,
  `-tail-tests.log`, `-doc-tests.log`.
- `cargo fmt --all -- --check`, `git diff --check`: passed.
- Runtime surface regenerated and logged:
  `f866137f3eb3150aec183d4bf2a976f2bc7383acec27bad70dafc1c35be2cbd2`.

## Coverage

A saved report is analyzed once with its key, interpretation and current target;
the result is revision + 1 of the same memory whose content is the exact user
quote (not the Agent text), with UserStatement evidence from the report source,
change `correction`, the old revision kept in history and no Owner action. More
steps and a reopened worker never re-analyze; the report still replays its
original ADR121 receipt after consumption. NoMemory, Unknown, Paused-then-retry,
an `update` change, two statements, an unresolvable quote and restart with
analysis in flight each end in the documented state without a revision, with
exactly one analyzer call (two for Paused). Deleted target or source closes the
job without analysis; a target revised while analysis is in flight is refused
without writing. End to end: an owned Runtime peer captures two user turns, the
hosted worker forms the first, an Agent saves a report quoting the second, and
the same worker automatically publishes the correction revision; a reopened
worker completes bookkeeping without another analysis.

## Not covered

No real correction model/adapter, prompt, budget or egress. Report-to-revision
linkage lives only in the private journal (chain records the revision and user
evidence). Journal loss before admission could re-analyze a report, as for
captured jobs. Supersede, forget, forget-hide (owner decision pending), MCP
transport and Runtime adoption remain. Synthetic tests are not owner acceptance.
