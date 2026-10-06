# MV-6 finding and correction — concurrent assertion replay

Date: 2026-10-05. Audited Core: `2a20a19`. Finding recorded at `5201cbb`. Correction: the subsequent feature commit containing this report. The finding was initially pending; the resolution below supersedes that status for the manual-assertion service only.

## Evidence

A temporary diagnostic used a synthetic Vault and an injected ID source to pause two identical manual-assertion calls after their initial receipt lookups but before either publication. Both calls then continued with the same text and key. The assertion that they return the same source ID failed: each returned its newly allocated ID. The store's identical-key replay publishes only one record; the losing call's ID is not the published receipt's source ID.

The diagnostic was run through the Core's private assertion helper, not the full `remember` page command. It demonstrates the service return-value race; it does not prove a particular UI failure or candidate outcome. The local reproducer and output are ignored under `.local/verification/assertion-replay-repro.rs` and `.local/verification/assertion-replay-repro.log`. The temporary test was removed by restoring the original source bytes in a `finally` block; `git diff --exit-code` confirmed that restoration.

## Cause and next bounded slice

`Vault::record_manual_assertion` handles receipt replay correctly at its initial lookup, but a replay returned by `commit` after concurrent publication is wrapped with the newly generated source ID. That result must instead use the source ID in the original receipt. Audit `start_session`, `append_user_message` and other service returns for the same pattern before broadening the fix. Preserve the existing format and writer protocol; add a permanent deterministic concurrency regression and a Core-level replay check, then run the repository's complete validation and record Runtime adoption.

The seven preceding MV-6 hardening features remain at `2a20a19`: 237 passing Rust tests, fmt, Clippy, independent 34-schema checking and logged Runtime surface verification. Those historical checks did not cover this race. MV-7 remains unopened. Real Runtime/UI behavior and durability remain unverified.

## Resolution

`record_manual_assertion` now takes its return ID from the published receipt when `commit` returns `Replayed`, rather than its discarded newly allocated ID. The deterministic reproducer is now permanent as `diagnostic_concurrent_assertion_returns_the_published_source` and passes. It exercises the service through the Core helper with two calls forced past their initial lookup before publication. No new format, dependency or product test hook was added. The full `remember` page race and other services remain separate follow-ups; hosts should continue to serialize same-key submissions.

The final complete suite passed **238 Rust tests** (31 Core tests), `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` on the pinned toolchain. The preserved complete logs contain no failures and end with all doc-tests and Clippy completed. Python 3.12 / python-jsonschema 4.26.0 passed all 34 schemas independently; the Runtime checker passed with aggregate `22676a68dfe16985678219347921a82ca711670cf6b27dd47aa964e0617fee49` logged. `git diff --check` passed. These checks supersede the earlier pending finding for this narrow correction.
