# MCP-4 durable correction-report intake

2026-11-03. Base `f6494cb` (implementation `ccbf1b1`). [ADR121](../adr/121-durable-agent-correction-reports.md)
and [intake contract](../contracts/AGENT_CORRECTION_INTAKE_V1.md).

The independent correction delegation now saves reports through the existing
witnessed Owner chain as a closed `agent_correction` delta and returns
`report_saved` original-key receipts. Preparation and submission share one
authorization gate; a saved key replays from the target's currently authorized
history after a System revision. Nothing is corrected, queued or Owner-reviewed.

Owned files: contract `next/agent_correction.rs`, `mod.rs`, `segmented.rs` and
its test; `agent-correction-v1` schema, catalog-v2 delta variant, synthetic
`tests/fixtures/agent-correction/cases.json` and Python cross-check; Vault
segmented `agent.rs`, new `correction_intake.rs`, `mod.rs`, `state.rs`,
`process.rs`; Host `qualified_agent.rs` and `tests/agent_correction.rs`; ADR121,
register, intake/native contracts, this report, Runtime log and regenerated
surface. Manual project status, AGENTS and main are unchanged.

## Checks on the final tree

Pinned Rust 1.98.1, offline/locked dependencies, serial Windows tests, unique
owned synthetic roots outside the restricted sandbox. No real Vault, Provider,
model, credentials, client configuration or Runtime checkout was opened.

- `cargo test -p enouia-memory-host --test agent_correction --locked`: **8 passed**
  (3 earlier draft tests plus 5 new intake tests).
- `cargo test --workspace --locked --no-fail-fast`: **909 passed, 0 failed,
  4 ignored, 110 targets**; log `target/correction-intake-workspace-tests.log`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed; log
  `target/correction-intake-workspace-clippy.log`. No Rust source changed after it.
- `cargo fmt --all -- --check`, `git diff --check`: passed.
- Python schema cross-check (existing venv, jsonschema 4.26.0): OK, 67 schemas,
  22 correction cases plus a valid `agent_correction` catalog segment.
- Runtime surface regenerated and logged:
  `773d2c95415bb7dfbcb8977d8df442c1c25ffc649c195055df7a118f2c6d96a4`.
  Combined log digest: `c25a4ded6314b4491eaefec92a632943b418cc95302877792443399aee3be801`.

An earlier full run on a pre-final tree had one failure,
`automatic_loop::continuation_cases::revocation_cancels_inflight_and_prevents_late_publication_or_refunds`
(OutcomeUnknown instead of Revoked under load). That older continuation worker
path is outside this slice; the test passed 3/3 in isolation and in the final
full run. It looks like an existing timing race, not proven fixed.

## Coverage

Report saved with draft key/hash and exact target; same packet replays without a
commit; changed packet conflicts; no memory revision, Owner action or queue entry.
After a lawful System revision of the target: preparation is Stale, the saved
key returns its original receipt, the changed packet still conflicts, an unsaved
key on the old target and an unknown revision are Stale, and a new report on the
current target saves. Chain replay on a fresh instance and fresh grant restores
both receipts; another Agent's identical packet is a different key.
After the revision, revocation, lock/unlock, expiry, source deletion, target
deletion, policy change and a fresh grant excluding the source all refuse
replay without writing. 32 reports per target, then Capacity before publication,
while the original still replays. Publication failure before staging and after
CURRENT returns OutcomeUnknown, latches the exact original request and blocks a
changed one; after explicit recovery (with a System revision in the persisted
case) the original yields exactly one receipt. Two threads on one reporter get
one receipt and one commit. Schema/native record cases reject Owner principals,
unknown fields, wrong/note keys, Assistant speaker, fabricated confirmation,
empty/oversized/whitespace interpretation, and receipts claiming correction,
queueing, memory results or sequences.

## Not covered

No System consumer evaluates reports; reports do not change recall. Supersede,
forget, forget-hide (owner decision pending), MCP tool/transport, real models,
private roots and Runtime adoption are outside this slice. Two independent
reporters racing on separate permits can see Busy/Stale (no cross-writer
serialization), as with notes. Synthetic tests are not owner acceptance.
