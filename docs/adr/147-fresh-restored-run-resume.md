# ADR-MEM-147 — Resume one fresh run on an explicitly activated restore

2026-11-16. ADR145 authenticates the original bootstrap home and operation.
ADR136 deliberately does not export that OS-account/canonical-home registration.
A relocated restore can retain the exact fresh journal but cannot use the original
CLI locator. Morii requested completion of this concrete continuation boundary.

## Decision

Add separate trusted native `resume-restored` and `recover-restored` commands.
The caller explicitly selects a protected durable Morii data home, its restored
child root and the exact verified export recorded by the restore receipt, with
`--bind-current-owner-self`. Reuse ADR110's current OS identity and protected-home
checks, and ADR136's restore receipt and export verification. Require the restore
to be activated already; this command cannot activate it or assert backup currency.
The selected root must be Morii-only and its immutable descriptor must match the
activated receipt/export. Parsed paths, descriptors and flags alone grant no
Morii authority; this entry is only for an already authorized trusted local CLI,
never MCP/page/Agent dispatch. The current native account owns the restored root,
as in ADR136; it does not recreate the old account or change logical Morii IDs.

Require the exact existing `fresh_run` binding before any recovery or initializer
open. Refuse missing journals, legacy/non-fresh runs and changed input. Never adopt
an empty restore, create a root, enroll a new run or replace persisted plans.
After the guard, open the existing initializer and retain its persisted plans,
packets, keys and bindings. Supplied input is revalidated and compared through
the existing whole-run/capacity route. Native opening/recovery still validates the
canonical chain and existing deletion/withdrawal gates.

`recover-restored` alone requests normal native forward recovery; resume does not
repair publication. Both keep receipt-only handling for an uncertain application.
A missing/withheld receipt remains unknown, including after policy expiry. No
replacement request, policy renewal, read grant or Morii action is manufactured.
Unstarted queued work still needs the original formation authority to be current.
No new on-disk registration, descriptor, journal format or restore receipt format.
The original bootstrap home/registration stays unchanged and cannot authorize a
second original root. Stop the old Host before restore activation as ADR136 requires.

## Verification and delivery

Use synthetic homes, explicit backup/activation, complete and interrupted runs,
source/attachment reads, changed-run and wrong-export refusals, quarantine and
outside-home checks. A native-account CLI rehearsal uses only a newly allocated
protected synthetic durable home. Publish synthetic evidence and a reviewable PR;
wait for Morii's merge before real restored-root use.

No real backup/restore, independently encrypted media, Runtime checkout, deployment,
data deletion, long-term read authority or new MV stage is included. Runtime has
no required UI/IPC adoption; only additive native helpers change its source surface.
