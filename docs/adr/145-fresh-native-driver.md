# ADR-MEM-145 — Explicit fresh input to native Owner formation

2026-11-15. This completes the driver between ADR140/142/143/144 and ADR110's
native Owner bootstrap. It introduces no analyzer, Provider, read grant or stage.

## Decision

Use a local `enouia-fresh-input/1` manifest containing exact shard selections,
explicit source identities and revisions (account, conversation, message, derived
Raw hash and text hash), sensitivity, attachment slots and supplied `AnalyzedBatch`.
Analysis indexes address the explicit source table, never parser discovery order.
Each file is explicitly named and SHA-256 bound; attachments additionally bind the
original parsed slot and its metadata through the selected Raw revision. Missing
attachments must be explicitly recorded as unavailable; external URLs remain text.
No directory scan, external fetch or invented attachment is allowed.

The exact manifest bytes and verified input hashes bind one complete run. Preserve
analysis actor, prompt and nullable model identity; format conversion is not new
analysis. Reuse selected conversation bytes, parser metadata, branch lineage,
every direct/context dependency, exact quotes, record times and entity dimensions.
Reject unused source-table entries, ambiguity, missing evidence and capacity
overflow rather than silently discarding them. Selected Raw contains the complete
conversation, including surrounding messages not represented by SourceRecords;
this is explicitly not redaction or authority to expose those bytes.

`plan <manifest>` reads only explicit inputs and builds/evaluates every bounded
plan in memory, including a conservative serialized whole-journal lifecycle bound
under 16 MiB. It creates no Vault, journal, policy, grant or network call. The
bound includes seeds, requests, escaped evaluation responses, bindings and receipts
for every work item, with maximum-width snapshot counters. A queued-only size is
not completion capacity. Overflow refuses before initialization/archive writes;
the limit is neither increased nor bypassed by deleting completed history.

`run <manifest> <owner-home> <bootstrap-operation> --bind-current-owner-self`
reauthenticates through the trusted local native bootstrap registration. Only its
recorded, exact root/descriptor is accepted; a descriptor copied into an arbitrary
root is insufficient. First adoption requires the native empty genesis check.
Root creation remains ADR110's explicit prepare/commit operation. Bind and enqueue
the complete run under the existing initializer lease before any capture/formation.
Resume accepts only the identical full run and retains its persisted plans,
packet, operation keys and bindings. Changed selection, analysis or attachment
bytes conflict, even when a new batch would otherwise fit. There is no append or
automatic second run on this root.

Reuse native capture, admission, witnessed writes, journal and receipt lookup.
An uncertain application only looks up its original binding. Missing receipts
remain blocked; no new generation or key is allocated. `recover` is an explicit
forward-recovery invocation for this same registered root, followed by the same
receipt-only barrier, never automatic resend. Formation policies have an explicit
seven-day interval; they create no long-term reading permission or Owner action.
Existing receipt authorization is preserved: an expired formation policy may
withhold the original receipt. That run remains blocked with unchanged bindings;
it is not evidence of non-commit and must never trigger a replacement request.

## Delivery boundary

All fixtures and verification are synthetic. Ship a reviewable PR and wait for
Morii's merge before applying the driver to real inputs or creating a real Vault.
Actual corpus capacity, native formation/reopen/query/source acceptance, independent
backup/restore and Runtime product acceptance remain separately evidenced gates.
No Runtime checkout, deployment, cleanup or manual status snapshot is changed.
