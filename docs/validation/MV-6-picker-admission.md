# MV-6 Core — picker admission

Date: 2026-10-05. Base: `c740c19`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, embedded Core and Runtime handoff. No storage format, provider, new product UI or stage change.

## Problem and reproduction

Picker lookup and successful consumption were separate critical sections. Ordinary page calls can run concurrently, so two calls could obtain the same single-use path before either consumed its token. This particularly affected unkeyed backup scheduling and restore verification; keyed import scheduling has its own replay guard but can overlap import preview.

A regression used channel-controlled admission and actual page requests. With the previous lookup/work/consume behavior, `backup_export` returned `operation_started` while that same token was held by another admission. The test failed expecting `busy`. An earlier run likewise admitted `import_preview`. The held work is synthetic; these checks demonstrate the lookup race and routing boundary, not two actual native dialogs or a Windows UI race.

## Change

All token-using routes share a per-token reservation. The picker map is locked only for lookup, reservation and release, never during command IO. A concurrent use returns existing retryable `busy` with rule `workspace.token_busy`; no paths are included. Different choices remain independent.

A scoped lease releases admission on error or unwind. Successful consuming commands remove the token. Import preview remains reusable, and failed root verification retains the selection until expiry. Import/backup scheduling consumes on successful scheduling, even if the background worker later fails. Reservations are retained during picker-map expiry pruning until their holder returns, but admission after the original ten-minute deadline is refused. The lifecycle gate continues to exclude close/root changes from ordinary page calls.

This does not add crash-persistent tokens or idempotency to unkeyed backup/create requests. A lost successful response still requires status/operation recovery or a new selection. Runtime adoption is recorded in the [handoff log](../integration/RUNTIME.md).

## Evidence

Four new synthetic tests cover:

- `picker_admission_excludes_concurrent_page_uses_of_one_token`: held admission rejects actual `backup_export`, `import_preview`, `import_start` and `restore_preview` requests with schema-validated retryable errors.
- `picker_admission_releases_on_error_and_unwind_and_consumes_on_success`: errors and caught synthetic unwind release the guard; reusable success keeps it and consuming success removes it. This tests the lease, not general panic recovery of arbitrary domain locks.
- `a_reserved_picker_does_not_block_other_choices_or_extend_its_ttl`: another registration/admission proceeds while a token is held; expiry remains the original deadline; active reservations survive pruning and become prunable after release.
- `picker_preview_retries_and_async_failure_preserve_consumption_rules`: missing preview input can be corrected; repeated preview works; successful import scheduling consumes despite later worker failure; rejected backup root retries; successful backup/restore consume; invalid restore retains its selection.

Existing root rejection, close cleanup, import replay and synthetic backup/restore tests remain in the Core suite. The focused Core suite passed 28 tests. Final pinned-toolchain checks passed: `cargo fmt --all -- --check`, all **235 Rust tests** in `cargo test --workspace --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Python 3.12 / python-jsonschema 4.26.0 passed the independent 34-schema cross-check. The Runtime surface checker passed with aggregate `27a5cf198c9cf5dcd641e87894cb979265af972365eb36a16cd1bbd383b851db` logged, and `git diff --check` passed. An initial complete-suite attempt stopped at the compatibility-log check before the new row was written; the final complete suite ran after that row was recorded.

## Limits

These are local synthetic Core/helper and command checks. Actual Runtime integration, native interaction, Windows accessibility and installer acceptance remain pending. No real memories, private attachments, provider/network calls, power-loss or real-data recovery were used or established.
