# MV-6 Core — different-key proposal deduplication

Date: 2026-10-06. Base: `b8b6678`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46, the existing governance proposal path consumed by Core. No wire/schema/dependency/storage-format change or new stage.

## Reproduction and correction

Two synthetic page requests used different keys with identical remember text and claim. Their manual sources were already saved. Both calls passed the pending-fingerprint lookup and paused at candidate allocation; one finished before the other resumed. The old Core published two pending candidates with the same fingerprint.

New candidate publication now carries the Vault head used for source/target validation and pending-fingerprint lookup as the existing commit precondition. The store rejects stale publication under its writer lock with `revision_conflict` / `fault.head_moved`; its writer protocol and persistent payload hashes are unchanged. Existing candidate edit/withdraw paths keep their record-revision preconditions and do not acquire this new head requirement.

Core makes one bounded proposal re-evaluation on that exact head-movement fault. It rereads the pending queue and target. An identical different-key proposal becomes a duplicate of the published candidate; a distinct proposal remains eligible to be stored. Existing evidence/sensitivity and target-revision checks still run. Repeated movement or other faults remain errors; this is not an unbounded retry policy or general governance request-binding change. Direct governance callers must handle the new optimistic head conflict themselves.

## Evidence

- `different_keys_do_not_publish_duplicate_pending_candidates`: full page responses succeed with one pending candidate and the same returned candidate ID. The old code failed with two pending candidates.
- `different_keys_preserve_distinct_proposals_after_head_movement`: the same allocation schedule uses different content. Both succeed with two distinct candidates after Core re-evaluation.
- `governance_refuses_proposal_publication_from_a_stale_dedupe_snapshot`: direct governance calls bypass Core recovery; one succeeds, the stale caller returns `HeadMoved` with no candidate result, and only one pending candidate is published.

A negative control removed only Core's new head-movement re-evaluation while retaining the governance precondition. Both page cases failed on the head conflict. Fixed source bytes were restored in `finally`, and SHA-256 equality confirmed. The three fixed focused cases passed. All sources, Vaults, actors and inputs are synthetic.

Required root checks passed with the pinned offline toolchain: formatting, 256 workspace Rust tests (49 Core tests), and all-targets Clippy with warnings denied. The independent Python 3.12 / python-jsonschema cross-check passed all 34 schemas and its fixture corpora. The Runtime surface checker passed with aggregate `d38dc307492b6bfeb6bb0b613c924b6225f9c002a1ea2af20840fb8d130e8ec9`; the adoption change is recorded in the compatibility log. `git diff --check` passed.

## Limits

These are controlled proposal-assembly schedules after source preparation. They do not establish every full remember/correction/forget execution, all policy/deletion races, indefinite contention recovery, edit-time fingerprint deduplication, duplicate-without-receipt request binding, actual Runtime adoption, real-data recovery or OS/power-loss durability. A duplicate response does not publish a receipt under its second request key; that earlier limitation remains. Runtime must keep host sequencing, use a key only for identical arguments, and refresh/re-plan after unresolved revision conflicts. MV-7 remains unopened.
