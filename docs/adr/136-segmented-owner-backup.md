# ADR-MEM-136 — Offline segmented Owner export and isolated restore

2026-11-11. Morii authorized this synthetic slice before legacy re-initialization.
The new root will be built and accepted locally, then moved to Linux. Real writes,
deployment and deletion remain separately authorized operations.

## Decision

- Add `enouia-memory-owner-backup export|verify|restore|activate`. Only
  `SegmentedOwnerV1` is supported. Stop the Host, initializer and worker first:
  export takes their exclusive leases and the writer lock. No implicit recovery.
- Export the descriptor, complete reachable commit chain, all archived raw objects
  (including unsupported inputs), initialization and captured-formation journals.
  Preserve receipts, queues, Owner actions, policy/deletion watermarks, authority
  and unknown outcomes. Exclude locks, orphan segments, indexes, staging, service
  configuration, OAuth state and credentials. Unknown journal files refuse export.
- Write a versioned manifest last, listing every file's size and SHA-256 and the
  pinned head. Independent verification checks the exact file set, safe paths,
  hashes, canonical commits, descriptor/genesis binding, existing admission replay
  and raw object closure. It never opens the source Vault.
- Restore only into an empty, verified, owner-only root outside the export. Verify
  before target writes, rebuild publication pointers, and publish CURRENT last.
  Preserve IDs and authority; never rebind the Owner or host.
- A durable restore receipt gates ordinary opening, including recovery. Native
  activation requires the Owner's explicit assertion that the selected backup is
  current, verifies that backup and all restored files, and requires the identical
  head. Restore a newer backup into another empty root rather than merge deletion
  ledgers. The Owner must establish that no later deletion, withdrawal or authority
  change is missing and stop the old Host before activating a migration copy.
- This portable plaintext layer works with the existing restic adapter, not new
  cryptography. A same-disk synthetic round trip proves recovery semantics, not
  independent disaster recovery. Real acceptance still requires a separate failure
  domain, encryption/recovery secret and a restore drill. Owner-data-home markers,
  bootstrap registration and original archives outside the Vault remain separately
  retained and are never reconstructed from guesses.

## Boundaries

No real data, model, network, Runtime checkout, deployment, cleanup or new MV
stage. Linux restoration must preserve the descriptor and use the account owning
the restricted root; cross-platform verification is recorded separately.
