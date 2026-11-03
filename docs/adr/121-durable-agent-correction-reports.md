# ADR-MEM-121 — Durable Agent correction reports

2026-11-03. MCP-4 continuation of [ADR120](120-native-agent-correction-drafts.md).

A prepared correction draft is data. Reports need a durable identity and an
honest original-key receipt without becoming a corrected memory, a second truth
journal or a queue the Owner must work through.

## Decision

Store an `agent_correction` delta in the existing Owner segmented commit chain.
The native System commits on behalf of the separately delegated Agent, reusing
the single writer, CURRENT/witness publication, uncertainty gate and forward
recovery. The record keeps report_key, request ID/hash, Agent, scope hash, exact
TargetRef, the evidence source's current PolicyRef, original QuotedEvidence and
the untrusted correction interpretation. Chain replay rechecks that the target
was the current undeleted revision and that the evidence was a current Runtime
User/UserStatement quote inside that active policy at commit time.

Submission takes the original request and AgentCorrectionPermit only. A draft,
NotePermit, read permit or MemoryPropose grant is never write authority, and
NativeMemoryOwner is not called. Authorization runs first on every call through
the Agent read gate: live permit, generation, current policies, fully qualified
current target memory and currently authorized exact evidence source. Deletion,
revocation, expiry and lock therefore refuse before any receipt is found.

Then the call takes one of two routes:

- **Original-key replay.** If the requested target is not current, its exact
  revision must still appear in the authorized selected history of that same
  currently qualified memory. A System revision after saving does not hide the
  original receipt; an unknown or unauthorized revision stays Stale/Denied.
- **New report.** The target must be the exact current revision, and the pinned
  quote locator plus strict original evidence checks apply as in ADR120.

Under the short publication lock the snapshot from authorization must still be
current, otherwise Stale. An existing report_key with the same request hash
returns the original receipt without a commit; a changed hash conflicts. A key
that was never saved for a non-current target is Stale. report_key keeps
ADR120's (capability, Vault, Agent, request ID) binding.

The receipt states `report_saved`: saved, not consumed, accepted, corrected or
queued for the Owner. Saving changes no source, memory, recall, queue or Owner
state. A later System consumer must evaluate the report through the existing
evaluator and a lawful target-bound commit, preserving actual source evidence,
conflicts and revision history; daily revisions stay automatic and manual
correction stays optional. That consumer, supersede, forget and MCP transport are
separate slices.

Capacity is at most 32 reports per target memory and MAX_SEGMENTED_REVISIONS per
root, checked before publication; saved receipts still replay at capacity.
Publication uncertainty closes the Vault instance; the Host reporter latches the
exact original request and revokes its permit. Explicit recovery plus fresh
native setup may repeat that exact request/key, including after a target change;
there is no automatic resend, reconnect or replacement key.

## Compatibility

Additive closed delta in schema 2 for Owner roots only; earlier binaries refuse a
root containing it. Do not downgrade such a root or open a second writer. Read
handshakes whose compatibility hash includes the catalog schema get a new
fingerprint; reachable read tools and authority do not expand. No MCP tool,
transport profile or forget-hide behavior is added.

See [intake contract](../contracts/AGENT_CORRECTION_INTAKE_V1.md).
