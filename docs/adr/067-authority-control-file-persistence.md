# ADR-MEM-67 — Durable authority control file, without data writes

Date: 2026-10-18. Scope: MV-8.1b, continuing owner-authorized MV-8.

## Decision

Implement a native, opt-in `host_authority::ControlStore` in the Vault crate for
authority-only transitions. Reuse verified roots, protected owner ACLs, managed
path checks, bounded reads and flushed atomic replacement. Hold both the existing
`indexes/host.lock` lease and Vault writer lock for the entire store lifetime.
Opening another Core or committing through the old CLI while these locks are
held must fail; process exit releases the locks.

`config/host-authority.json` is an additive pilot control file. Bootstrap is an
explicit native operation, allowed only once for a fresh epoch-1 authority with
matching Vault identity and independently verified checkpoint. Open never
initializes missing/corrupt files. Every operation rereads and validates the file
and the adapter's checkpoint; no cached authority or default primary is used.
Retained promotion receipts recover an exact operation after a lost response.
An ambiguous write failure returns outcome_unknown and requires rereading.

The native checkpoint reader remains a port: a fixture reader is not a complete
snapshot verifier. Owner authorization, target restoration/recovery evidence and
authenticated target identity remain the calling native control plane's duties.
No page, IPC or CLI entry point is added in this slice.

## Strict boundary

ControlStore is NOT an implementation of the full AuthorityStore transaction
port. It only accepts quiesce/promote/abort; ordinary data advancement has no
entry point. Holding locks and atomically replacing one metadata file does not
make metadata and CURRENT one transaction. A later data coordinator must solve
their shared publication and recovery boundary before opening data writes.

Today's embedded Core/CLI do not read this file after the control-store lease
ends. Persistent or cross-host fencing therefore remains unactivated. This
library neither migrates a Vault nor permits production cutover. Real file and
child-process tests establish only the explicitly observed Windows synthetic
control-file/lock behavior, never Linux/cloud support or power-loss durability.
