# MV-6 Core — import preview input admission

Date: 2026-10-06. Base: `baa96b8`. End: the feature commit containing this report. Scope: the existing Core `import_preview` route consumed by Runtime. No new stage, wire/schema/API/dependency/storage-format change or modification of the import writer.

## Reproduction and correction

A synthetic file was selected through the native-token registration boundary, then replaced with a directory before the page requested a preview. The old Core used following metadata and an unrestricted file read, returning `storage_failed` / `workspace.pick_unreadable` rather than rejecting the changed selection's type.

Preview now uses non-following metadata at command use and refuses non-files or the existing platform helper's reparse points with `invalid_request` / `import.input_not_regular_file`. Missing and unreadable selections keep their existing errors. The open file is read with a limit of the smaller of the observed length and the existing global import budget, plus one sentinel byte. Exceeding the global budget is `import.input_too_large`; a differing observed length is `import.input_changed`. These are existing import rule names now used by Core preview. Successful unchanged previews still hash/detect the current bytes and leave the reusable token available.

This bounds actual bytes read when input length changes after the metadata check. It is not an atomic file-identity check, a complete defense against concurrent path replacement, or a bound on every parser allocation. The underlying import writer's independent read protocol is unchanged.

## Evidence

- `import_preview_revalidates_a_changed_picker_file_kind`: an actual synthetic file-to-directory replacement is rejected with no result. Restoring a regular file allows the same token to preview twice; the Vault head does not move. The old Core failed this test.
- `import_preview_bounds_actual_reads_past_the_metadata_budget`: an exact-limit synthetic reader succeeds. An 8192-byte reader under a nine-byte budget is refused after reading ten bytes, rather than reading the entire input.
- `import_preview_refuses_size_drift_within_the_global_limit`: growth below a larger global limit is refused after the metadata budget plus one byte; shorter input is also refused with `import.input_changed`.

A negative control removed only the reader limit, retaining length/error checks. The resource-bound regression failed because 8192 bytes were consumed instead of ten. Source bytes were restored in `finally`, and SHA-256 equality was verified. The three fixed focused cases and the existing picker retry/async-consumption regression passed.

File symlink creation in this Windows environment required administrator privileges, so no real file-symlink or concurrent reparse replacement acceptance is claimed. The executed type-change case uses a directory; the reparse branch reuses the existing platform metadata predicate.

Required root checks passed with the pinned offline toolchain: formatting, all 263 workspace Rust tests (56 Core tests), and all-targets Clippy with warnings denied. Python 3.12 / python-jsonschema independently passed all 34 schemas and fixture corpora. The regenerated, logged Runtime aggregate is `2ed4aa7d1396175974e21f1d88173e7085180e74868c928d03995553caac1be3`; the surface checker and `git diff --check` passed.

## Runtime adoption and limits

Runtime must bump the pin, explain the three preview import rules as invalid or changed input, and let the owner retry or re-pick after restoring/reselecting it. These refusals retain the token until normal expiry; a preview success remains reusable. Continue to serialize token submissions. All inputs and Vaults are synthetic. Actual Runtime behavior, all filesystem races, real-data recovery and OS/power-loss durability remain unverified. MV-7 remains unopened.
