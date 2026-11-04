# ADR-MEM-116 — Durable Agent hint intake

2026-11-03. Continuation of [ADR115](115-native-agent-note-drafts.md).

Validated drafts alone do not deliver a hint to the System. Hints need a durable
identity and an honest result, while remaining separate from captured sources
and formed memories.

## Decision

Store an `agent_note` delta in the existing Owner segmented commit chain. This
uses the existing single writer, CURRENT/witness publication, uncertainty gate
and forward recovery. It creates no second authoritative journal or writer.
The native System commits on behalf of the explicitly delegated Agent; the
record preserves that Agent, exact quote/role/class, request hash and scope.

Submission accepts the original request and the separate note permit, prepares
against current authorization, then requires the same snapshot under the short
publication lock. A changed head returns Stale before publication. Lock/revoke
and expiry still apply. A serialized draft is never accepted as authority.

`note_key` remains bound to capability, Vault, Agent and request ID. Under that
key, the same request hash returns the original receipt without another commit;
a changed hash conflicts. Current source authorization precedes replay, so a
receipt cannot recover deleted or revoked source content. Each source accepts
at most 32 hints; the root has the existing segmented revision ceiling.

The receipt says `hint_saved`, never queued, consumed, approved or formed. Saving
does not create a source, schedule or repeat analysis, create memory, grant an
Owner action or hold a conversation for human review. System consumers may use
or ignore still-current hints separately. Hints arriving after dispatch do not
authorize another model request.

Publication uncertainty closes the existing Vault instance. Recover the chain
and explicitly repeat the exact original request/key under fresh native setup;
do not auto-resend or mint a replacement key. No receipt comes from unconfirmed
publication. Recovery preserves original note and commit identities.

## Compatibility

This is an additive closed delta variant in schema 2 for Owner roots only.
Earlier binaries refuse a root containing it during chain decoding; do not
downgrade that root to an older binary or open a second writer. Existing old
root writers, pipe-1/2, Agent read profiles and Workspace IPC58 retain their
meanings. No MCP tool or write transport is advertised by this native slice.
Read handshakes whose compatibility hash includes the complete catalog schema
have a new fingerprint; use matching client/server revisions. Their reachable
read-tool schemas and authority do not expand to note submission.

See the [submission contract](../contracts/AGENT_NOTE_INTAKE_V1.md). Real clients,
model egress, private data and Runtime adoption remain separate.
