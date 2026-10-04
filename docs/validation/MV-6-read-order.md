# MV-6 latest search results and stable pagination

Date: 2026-10-04. Base: `d7dc86c`. Scope: Explorer read requests only. Write idempotency and Core authorization are unchanged. MV-7 remains unstarted.

## Reproduction and correction

A separately built fixture mounts the production Explorer with synthetic deferred read responses. It demonstrated five failures: an earlier success overwrote the latest results; an earlier error reappeared after a newer success; an earlier completion cleared the current read's busy state; pagination used an edited but unsubmitted query with a previous cursor; and the history filter switched a search to the unfiltered list. The baseline passed only the append and empty-result cases (**2/7**).

Explorer now publishes results, errors, and busy state only for its latest read generation. Unmount invalidates pending generations. The query submitted for a result set is retained separately from the input draft, so pagination, history filtering, and retry use the same query. New searches remain possible while an earlier read is pending. The list exposes its current busy state and a reading announcement. Writes continue to use the existing action/key mechanism.

The test read client is supplied only by the independent fixture. The production Explorer defaults to the existing Core IPC client; no native fault command, replacement IPC, new permission, model request, or real memory data is introduced. The fixture is built under ignored local material and is absent from the production frontend.

## Verified

- The same controlled-response fixture passed **9/9** checks: the five corrected failures, current-page append, clearing previous rows for an empty result, retry retaining the submitted query despite input edits, and retry recovery with the busy/error state cleared.
- Frontend and both read/retry fixture typechecks and builds passed. The rebuilt production application and NSIS installer remain version 0.1.0.
- The final real-app run on a fresh synthetic Vault passed **43/43** checks, including real Core import/review/correction, sessions, context inspection, keyboard focus, overlay restrictions, original write-key reuse, actual renderer crash, and acknowledged-record recovery.
- Current-day pinned offline format checking, **196 Rust workspace tests**, Clippy with warnings denied and the independent **34-schema** Python cross-check remain applicable to unchanged Rust/Core sources. The new ordering fixture is UI simulation evidence, while the 43-check run uses actual IPC and Core; neither is real-data or real-model acceptance.

## Limits

This change covers Explorer search/list ordering, not every other screen's asynchronous navigation. Previously published manual accessibility/login/upgrade/signing gaps remain pending. No real exports, Vaults, credentials, test bundles or installer artifacts are committed.
