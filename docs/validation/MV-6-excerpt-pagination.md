# MV-6 Core — source excerpt pagination

Date: 2026-10-05. Base: `8adb51b`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, source excerpt behavior and Runtime handoff. No storage format, provider, product UI or stage change.

## Problem and reproduction

The workspace contract permits a `maxBytes` budget from 1 to 8192. UTF-8 boundary rounding could reduce a small budget to zero bytes while source text remained. A caller requesting the next page at `byteEnd` would then repeat the same page indefinitely. The reference UI uses 4096 bytes, so this finding concerns other valid contract callers; an actual reference-window loop was not observed.

A synthetic manual source containing `Aé中😀Z` reproduced the problem through the schema-validated page channel. At offset 1 with budget 1, the previous Core returned a successful empty excerpt with `byteStart = byteEnd = 1`, `totalBytes = 11` and `truncated = true`. The new regression failed on that response before the fix.

## Change

If no character fits before EOF, `source_excerpt` returns existing non-retryable `invalid_request` with rule `workspace.excerpt_budget`. The caller must increase the byte budget. It never exceeds the cap to force progress. Successful pages before EOF advance `byteEnd`; empty source/EOF results and rounding a start inside a character back to its boundary remain unchanged.

The public Core API, workspace schema and helper signature are unchanged. Runtime must adopt the documented behavior with its revision bump; a budget of at least four bytes fits one UTF-8 character. The existing reference UI budget needs no change. The [handoff log](../integration/RUNTIME.md) records the new surface.

## Evidence

`source_excerpt_advances_or_rejects_an_insufficient_utf8_budget` covers 56 schema-validated requests over ASCII, two-byte Latin, three-byte Chinese and four-byte emoji: budgets 1–4, every offset through one byte beyond EOF, and the largest contract-safe integer. It checks correct errors, successful text slices, UTF-8 boundaries, byte caps, cursor progress, totals and EOF responses. An initial test input using `u64::MAX` was correctly refused by the existing safe-number contract; the final test uses that contract's `MAX_SAFE_INTEGER`.

Final checks on the pinned toolchain passed: `cargo fmt --all -- --check`, all **236 Rust tests** in `cargo test --workspace --locked` (29 Core tests), and `cargo clippy --workspace --all-targets --locked -- -D warnings`. The independent Python 3.12 / python-jsonschema 4.26.0 cross-check passed all 34 schemas. The Runtime surface checker passed with aggregate `c8c4a1ff482c8449469664aad537adc1a4888ce68a79aed793a690d9851cf7a2` recorded in the handoff log; `git diff --check` passed.

## Limits

This is synthetic Core/contract evidence. It does not establish actual Runtime adoption, native UI behavior, real imports, provider calls, Windows accessibility, installer acceptance or power-loss durability. UTF-8 character boundaries are not grapheme boundaries; multi-code-point glyphs can span pages as before.
