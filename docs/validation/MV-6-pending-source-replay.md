# MV-6 pending finding — concurrent assertion replay

Date: 2026-10-05. Audited Core: `2a20a19`. Status: **reproduced, not fixed**. This report is not a completed feature or additional passing test.

## Evidence

A temporary diagnostic used a synthetic Vault and an injected ID source to pause two identical manual-assertion calls after their initial receipt lookups but before either publication. Both calls then continued with the same text and key. The assertion that they return the same source ID failed: each returned its newly allocated ID. The store's identical-key replay publishes only one record; the losing call's ID is not the published receipt's source ID.

The diagnostic was run through the Core's private assertion helper, not the full `remember` page command. It demonstrates the service return-value race; it does not prove a particular UI failure or candidate outcome. The local reproducer and output are ignored under `.local/verification/assertion-replay-repro.rs` and `.local/verification/assertion-replay-repro.log`. The temporary test was removed by restoring the original source bytes in a `finally` block; `git diff --exit-code` confirmed that restoration.

## Cause and next bounded slice

`Vault::record_manual_assertion` handles receipt replay correctly at its initial lookup, but a replay returned by `commit` after concurrent publication is wrapped with the newly generated source ID. That result must instead use the source ID in the original receipt. Audit `record_agent_submission` and other service returns for the same pattern before broadening the fix. Preserve the existing format and writer protocol; add a permanent deterministic concurrency regression and a Core-level replay check, then run the repository's complete validation and record Runtime adoption.

The seven completed MV-6 hardening features remain at `2a20a19`: 237 passing Rust tests, fmt, Clippy, independent 34-schema checking and logged Runtime surface verification. Those checks establish their recorded scope; they do not cover this newly demonstrated race. Until corrected, hosts should serialize same-key source submissions in addition to preserving identical retry input. MV-7 remains unopened. Real Runtime/UI behavior and durability remain unverified.
