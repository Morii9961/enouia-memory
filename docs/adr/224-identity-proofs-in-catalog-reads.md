# ADR-MEM-224 — Catalog reads accept Morii's entity identity proofs

Date: 2026-11-27. Status: Accepted (fix). Corrects ADR-MEM-172, which
added `entity_identity` owner actions but let only the graph page accept
their proofs.

## Context

When Morii confirms that entity records are one identity
(`OwnerAction::EntityIdentity`, same = true), the vault redirects each
memory's mention of a non-canonical record and attaches the action's proof
(`OwnerActionProof` with action `entity_identity`). The catalog item, the
neighbor relation and the conversation summary still accepted only
`entity_merge` and `entity_split` proofs. Their schemas already allowed
`entity_identity`.

Runtime found this on the real library. After Morii confirmed five records
as one identity, every catalog page holding one of the four affected
memories, and their detail, returned `Invalid`: the Vault list stopped at
page 6 and 347 of 351 memories could be read. Recall was not affected,
because it does not build catalog items.

## Decision

The native checks of `CatalogReadItem` entities, `NativeNeighborRelation`
owner relations and the scoped conversation summary accept `entity_identity`
proofs, as `MemoryGraphPage` already did. Nothing else changes: the data
was valid, and the readers refused their own output.

## Verification

`identical_names_link_as_suggestions_until_morii_confirms_or_separates`
now reads the catalog page, every memory's detail and its neighbors before
and after Morii confirms and separates records. Before this fix the page
read after the confirmation failed with `Invalid`; now it succeeds and
carries the `entity_identity` proof.
