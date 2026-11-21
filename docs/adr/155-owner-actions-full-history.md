# ADR-MEM-155 — Morii's own actions work on his full history

Date: 2026-11-21. Status: Accepted (Morii authorized R4.3 writes); synthetic validation.

## Context

ADR-MEM-153 lets Morii read every memory after its formation window ends. His
own actions (correct, withdraw, forget, protect, tag, theme rename/pin/dissolve,
entity merge/split) still planned, confirmed and replayed against the
current-policy scope. After a formation window ended, those memories disappeared
from that scope. Any action on them was refused, and an action confirmed just
before a window ended failed at commit. Runtime R4.3 puts these actions on the
desktop, so this would block nearly every write on the real Vault once its
fresh-initialization policies expire.

## Decision

New owner action plans, confirmations and outcome reads use the
`MoriiFullHistory` read basis from ADR-MEM-153. Policy checks inside the effects
use each memory's own recorded policy revision (`read_policy` with the
independent-read flag). They no longer use only the exact current policy. So the
entity-merge and correction boundaries still hold: a memory cannot borrow an
entity or source outside its own policy.

Replay stays deterministic without a format change. The recorded
`OwnerActionEffects.scope_hash` already binds the scope. When admitting an owner
commit, the Vault recomputes the scope under the current-policy basis and then
under the full-history basis, and accepts the one that reproduces the recorded
hash. It then recomputes the effects under that same basis and requires them to
be equal. Actions committed before this change, such as the real Vault's
2026-11-19 correction, still replay under the current-policy basis.

Deletion, withdrawal and forget barriers, the sensitivity ceiling, snapshot
binding, the five-minute plan expiry, the trusted-surface confirmation and the
single atomic commit are unchanged. Agent, MCP and remote writes are not
affected.

## Consequences

- Once a full-history owner commit exists, binaries without this change refuse
  to open the Vault. Runtime and the backup tool must move to this revision before
  the first such write. An export's `verify` replays admission, so it must also
  use a binary that includes this change.
- The existing test that expected a formation window ending between
  confirmation and commit to refuse Morii's protect action now expects the
  commit to succeed. The foreign-policy entity-merge refusal in the same test is
  unchanged.
- No persisted format, IPC command or wire change.

## Verification

- A synthetic Vault with two memories, past the fixture formation windows:
  correcting one and withdrawing the other both succeed. After reopen (full
  replay), both outcomes are returned, and the full-history catalog shows only the
  corrected memory at revision 2.
- All host owner tests pass.
- The real seq 440 export, whose last commit is a current-policy correction,
  verifies with the new binary.
