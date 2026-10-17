# ADR-MEM-65 — Recorded context source dependencies

Date: 2026-10-17. Scope: MV-7 semantic proposal lifecycle and generic Core
dependency enforcement; not cloud migration or a new MV stage.

## Problem

The synthetic independent-source reproduction retains a pending derived
candidate after purging uncited context that the model actually read. Shared
export objects hid this gap in the earlier purge fixture. Sensitivity already
uses every selected source; evidence contains only quoted supporting sources.
Do not turn uncited context into supporting evidence to repair deletion.

## Decision

Add optional context_source_refs to CandidateRecord and CanonicalMemory, as a
bounded unique array of SourceRevisionRef (at most 128). These are recorded
dependency anchors, never evidence, permission, corroboration or completeness.
Legacy absence means no recorded context dependency, not proof of no dependency;
omit empty arrays when serializing so unchanged legacy records stay unchanged.
No private-data scan or automatic reconstruction of old records is authorized.

Proposal carries these references separately from evidence. Resolve them through
the Vault, include their sensitivity in the floor and declassification binding,
and validate their closure in full and delta validation. Identity proposals do
not accept this field. Native semantic drafts record all explicitly selected
source revisions, retaining exact supporting citations separately in reason and
evidence. Ordinary deduplication still returns an old record without appending
new context or explanation; do not fabricate new provenance on a duplicate.

Approval copies dependencies to the canonical memory; merges/revisions preserve
the union. Context compilation includes them in provenance/resource freshness,
without adding them to a memory item's supporting evidence. Purge considers
every candidate revision's recorded dependencies. Other canonical memories
follow the existing owner-visible losing-provenance policy, including context
dependencies; saved capsules/derived session text follow normal purge closure.

The field is security-relevant and must be an explicit schema property, never an
extension or a parsed instruction from reason text. This is additive for this
reader, but older strict readers cannot open records containing the new field.
Record the Runtime surface change and required pin adoption in the same commit.
Do not modify real Vaults, activate Runtime, migrate data, send requests or
approve real memories. Real acceptance and legacy dependency recovery remain
separate owner-controlled work.

## Evidence

[Reproduction and validation](../validation/MV-7-semantic-context-dependencies.md).
