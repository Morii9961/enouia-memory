# MV-6 Core — late proposal receipt recovery

Date: 2026-10-06. Base: `32f43d4`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, owner proposal admission through Core. No wire, schema, dependency, storage-format or stage change.

## Reproduction and correction

A synthetic manual source is saved first, representing a partial remember operation before proposal publication. Two identical page requests then pass the initial proposal receipt lookup. The ID port pauses both at candidate allocation; one is released to finish before the other resumes. The old Core refused the later identical request with `idempotency_conflict` / `fault.idempotency_conflict`: governance hashes generated candidate bytes, which differ across the two allocations.

Core now shares owner proposal admission across remember, correction and forgetting. Only on this conflict does it inspect a published receipt's original candidate revision and compare logical proposal fields (kind, type, content, details, targets, expected revision and reason). A mismatch is refused with `workspace.key_reuse`. A matching proposal gets one bounded retry through the existing governance receipt lookup. Other failures pass through; route-specific pre/post checks remain. Persistent hashes and writer behavior are unchanged, and there is no general page retry loop.

## Evidence

- `remember_replays_a_proposal_published_after_its_initial_lookup`: both schema-valid page responses agree, exactly one pending candidate exists, and subsequent replay or rejected changed-claim reuse does not move the head. This failed on the prior Core and passed after correction.
- `remember_refuses_a_changed_claim_published_after_its_initial_lookup`: the same paused allocation schedule uses two different claims. Exactly one succeeds; the other returns `idempotency_conflict` / `workspace.key_reuse` with no result, leaving one pending candidate.

The earlier correction/forget request-binding regressions and Context/Mock/full-session replay checks passed in the focused replay run before the second negative-choice case was added; both final admission cases passed separately. All inputs and roots are synthetic.

Final root checks with the pinned toolchain passed: `cargo fmt --all -- --check`, all 252 workspace Rust tests (Core 45), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Python 3.12 / jsonschema 4.26.0 passed all 34 schemas independently. The regenerated, logged integration aggregate is `88c1db214d2480dc9dca5d610e03022533b0093c1677cc0c04aa576cfeef4af8`; the surface checker and `git diff --check` passed.

## Limits

The new forced positive/negative schedules exercise remember after source creation, not every whole remember or correction/forget concurrency interleaving. The shared helper does not replace general governance request binding or make source/proposal creation atomic. Duplicate-without-receipt, different-key contention, Runtime adoption, actual Windows/UI behavior, real-data recovery and OS/power-loss durability remain separate gates. Runtime must retain host sequencing and change keys when logical arguments change. MV-7 remains unopened.
