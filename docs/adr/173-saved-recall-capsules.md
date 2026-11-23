# ADR-MEM-173 — Saved recall capsules on the registered Vault

Date: 2026-11-23. Status: Accepted (Runtime R5 request B1; use on the real Vault
is Morii's choice); synthetic validation.

## Context

Runtime's Context surface has a capsule index, and its recall experiment has a
"save" button. The legacy v1 store has capsule records (ADR-MEM-25), but the
registered, segmented Vault has none. Its native recall preview (ADR-MEM-152)
deliberately writes nothing.

## Decision

A saved capsule is one of Morii's owner actions, so it uses the existing native
plan, confirmation and commit path and adds no new record type.

- **Action.** `OwnerAction::Capsule { name, query, members }`:
  - `name` is 1–60 characters.
  - `query` is the recall text he ran, at most 2000 characters.
  - `members` is the returned memories in order, at most 128 distinct targets.
    Members are checked at planning like tag targets.
  - Saving an existing name replaces that capsule. Saving with no members
    discards it.
- **Listing.** `NativeMemoryOwner::capsules(max)` returns `SavedCapsules {
  snapshot, capsules }`, the latest version per name in name order. Each
  capsule carries its query, the `saved_by` proof (action `capsule`) and its
  members. Each member lists its `saved` revision and its `current` revision in
  Morii's full-history read at `max`; `current` is `None` when the memory has
  been withdrawn, forgotten or deleted since. The listing reads Morii's own
  action records unfiltered, so deleting a member never brings back an older
  version.
- **Visibility.** In other reads a capsule action is visible only when one of
  its members is, and it yields no marks or proofs there.

Runtime keeps the preview's scores and reasons itself, or reruns the query. The
capsule records what Morii saved, not a frozen ranking.

## Consequences

- Runtime can save and list capsules on the registered Vault. Whether to enable
  this on the real Vault is Morii's decision (Runtime R5), so it stays off
  until he chooses.
- The contract adds an owner action and the proof name `capsule`. Older binaries
  refuse a Vault that contains it.
- No persisted format change beyond the new action variant. No IPC command or
  wire change.

## Verification

- **Planning.** An empty name, a name of 61 characters and duplicate members
  are refused.
- **Listing.** Two capsules are listed by name, with order, query and proof
  kept.
- **Replacement.** Saving a name again replaces it.
- **Deletion.** After its member is deleted, the capsule still lists the saved
  revision with no current revision; the older version does not return.
- **Discard.** Saving with no members removes the capsule.
