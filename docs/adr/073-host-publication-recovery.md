# ADR-MEM-73 — Witnessed Host publication and explicit forward recovery

Date: 2026-10-21; verified 2026-10-22. Scope: owner-authorized MV-8.1d native library.

CURRENT alone cannot identify the last acknowledged authority/data state after
it is damaged. Searching for an older valid manifest could roll back deletion
or primary watermarks. Recovery therefore needs durable publication evidence,
not a guessed directory order.

## Publication

Use opt-in native `host-current-v2` mode. Before each Host CURRENT replacement,
flush one closed bounded publication intent: the exact before/after pointers
and the after AuthorityState. Before-authority bytes remain immutable on disk.
Validate the legal transition and actual target data closure. Initial bootstrap
writes this intent before its mode marker or authority directory; intent
presence also fences every legacy mutation/export and the authority-only pilot.
After CURRENT publication and the existing best-effort journal append, flush a witness selecting the after
pointer before returning success. Ordinary Host operations require CURRENT,
witness and the latest intent's after pointer to agree. Partial publication
refuses further writes until explicit recovery.

Existing ordinary legacy roots remain unchanged. This replaces v1 of the
opt-in native prototype: v1 Host-marked roots
are refused, never silently upgraded, reset or treated as ordinary roots. The
authority-only pilot remains separate and cannot open an intent-marked root.

## Recovery

A native recovery coordinator holds the Host lease and uses the writer lock.
Its plan binds the exact intent, observed control-file bytes, native after-primary
identity and verified target checkpoint. The previous witness must select the
intent's before or after pointer; absence is allowed only for an initial
bootstrap. A valid unrelated CURRENT, missing ordinary witness, invalid intent,
missing before-authority or damaged data refuses recovery. Do not guess another
checkpoint, rewrite content or lower policy/deletion/primary watermarks.

Owner confirmation binds that plan's exact hash, including the observed mode
and selected after-authority file hashes. Missing/torn mode or after-authority
metadata may be atomically restored to the exact verified intent bytes;
explicitly different mode versions, missing/corrupt before-authority, corrupt
intent/witness or damaged data remain refused. Recovery may finish only this
latest validated intent, writing its marker/authority/CURRENT/witness as needed.
It creates no new data commit and preserves the original operation receipts.
An exact completed repair replays without writing. Retries may recognize the
same plan's exact target control bytes produced by partial repair; unrelated
observation changes conflict. Open/read never repairs and native approval or
identity may not come from page/MCP arguments.

## Boundaries

This is forward recovery for the current protected local publication files,
with synthetic Windows I/O/process-abort evidence. It is not independent backup
restoration, protection against external rollback of the whole filesystem,
distributed consensus, a Host executable, authenticated transport, Runtime
adoption, real migration or power-loss proof. Missing/corrupt authoritative
witnesses remain unavailable pending a separately designed restore procedure.
