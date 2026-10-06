# MV-6 Core dependency — import read budgets

Date: 2026-10-06. Base: `f1be1df`. End: the feature commit containing this report. Scope: the existing import pipeline used by Core's background `import_start` worker. No new stage, public API/wire/schema/dependency/storage-format or archival protocol change.

## Reproduction and correction

The import pipeline checked regular-file metadata and a global size limit, then read the entire selected path twice before comparing length and hashes. It refused changed input, but only after consuming any growth in either read. A private reader factory now exposes those existing open boundaries to deterministic tests; production uses ordinary `File::open` with no test hook or global state.

Actual synthetic files were changed after metadata, immediately before the first or second open. In the old unbounded path, the second open consumed 8192 bytes for a nine-byte observation; the first-growth case unnecessarily opened the file twice before refusing it. The regression cases failed before correction.

Each read now stops at the smaller of the observed length and global limit, plus one sentinel byte. A size mismatch is refused immediately with existing `import.input_changed`; `import.input_too_large` remains the global-budget refusal. Only matching lengths reach the existing two-read SHA-256 comparison. Same-size content replacement still fails that hash comparison. Stable input keeps both exact reads and the bytes returned for archiving are unchanged. Nothing is archived before this admission finishes.

## Evidence

- `stable_input_keeps_both_exact_reads`: a real nine-byte synthetic file returns the exact bytes, with eighteen bytes read across two opens.
- `changed_second_input_read_stops_at_the_observed_budget`: deterministic replacement before the second open covers growth, shrinkage and same-size different content. All return the original changed-input rule, and the second read consumes at most ten bytes.
- `changed_first_input_read_stops_before_opening_again`: replacement before the first open is refused after ten bytes and exactly one open.
- `oversized_input_is_refused_before_opening`: metadata above the configured global budget prevents any open.
- `import_worker_refuses_changed_kind_without_a_partial_archive`: a selected file becomes a directory before the full page starts its worker. The terminal task fails with the existing regular-file rule, has no result/import manifest, and leaves the Vault head unchanged. Scheduling consumes the token as before; restoring the file and selecting it again allows a successful import.

These fixed focused cases passed. The growth tests alter real synthetic files at injected open boundaries; they are controlled schedules rather than general OS-race acceptance.

The required root suite passed all 268 Rust tests (57 Core tests), and formatting passed with the pinned offline toolchain. Clippy first rejected the new test module's placement before production items; moving that unchanged module to the file end resolved the lint. Its four focused cases passed again, followed by all-targets workspace Clippy with warnings denied. Python 3.12 / python-jsonschema independently passed all 34 schemas and fixture corpora. The regenerated, logged Runtime aggregate is `683fa7bbb350698aa74c5dd9bc6d6f25c18f6e58221ccd50c75eecb497fcb33d`; the surface checker and `git diff --check` passed.

## Runtime adoption and limits

Adopt this deeper import implementation together with the pin bump, reviewing the domain diff as required by the integration runbook. Keep terminal task errors distinct from an accepted cancellation request, and re-pick after a consumed import token. This change bounds selected-file reads; it does not cap every parser allocation, provide an atomic path identity/no-follow guarantee, or change resume/archive/receipt semantics. Actual Runtime, real exports, all reparse races and OS/power-loss durability remain unverified. MV-7 remains unopened.
