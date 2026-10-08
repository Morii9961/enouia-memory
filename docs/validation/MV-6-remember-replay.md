# MV-6 Core — remember replay binding

Date: 2026-10-05. Base: `bd25c4d`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, embedded Core request behavior and Runtime handoff. No storage format or stage change.

## Problem and reproduction

Manual assertion receipts bind text, while governance proposal replay finds the previous candidate by key without comparing the proposal input. A Core `remember` retry with identical text but a changed `claimKey` therefore returned the previous candidate successfully. The regression reproduced this after accepting the candidate: the old Core answered `candidate_proposed`, revision 2, with no error for the different claim.

## Change

Core checks an existing remember proposal receipt before and after proposal admission. It reads the original candidate revision named by that receipt and compares both text and claim. A mismatch returns existing `idempotency_conflict` with `workspace.key_reuse`. The postcheck covers a receipt first published between the precheck and proposal admission; it does not make source/proposal creation one transaction.

The original revision is read through the existing Vault historical-read API and its purge barriers; there is no new persistent metadata, receipt hash, format migration or cache. Identical retries survive review and reopen. Runtime must keep a key only for identical input, and refresh current review state after replay. The [handoff log](../integration/RUNTIME.md) records adoption requirements.

## Evidence

`remember_replay_binds_the_claim_key_after_reopen_and_review` records synthetic text, accepts its candidate, and checks identical versus changed-claim retries both before and after Core shutdown/reopen. Identical requests return the same candidate; changed claims conflict; the published head remains unchanged. The test failed on the previous Core's success response and passed after the fix. This is an in-process synthetic reopen test, not an OS restart or power-loss test.

Final checks on the pinned toolchain passed: `cargo fmt --all -- --check`, all **237 Rust tests** in `cargo test --workspace --locked` (30 Core tests), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Python 3.12 / python-jsonschema 4.26.0 passed all 34 schemas independently. The Runtime surface checker passed with aggregate `e493a75c9018391b353ae3f03a674297c08aba9a6bc35c5c10e5e1b8a33116cf` recorded in the handoff log; `git diff --check` passed.

## Limits and follow-up

This guard covers successfully published Core `remember` proposals. The source and proposal remain separate commits. A failure after the source commit but before proposal publication does not yet bind a claim key. Governance `propose`, `edit_candidate` and `withdraw_candidate` replay still use key-only lookup; complete domain request binding needs its own design and tests. No general idempotency, concurrent source-creation, migration, Runtime adoption, real-data or durability claim follows from this slice.
