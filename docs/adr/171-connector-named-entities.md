# ADR-MEM-171 — Named entities in ChatGPT-distilled memories

Date: 2026-11-23. Status: Accepted (first part of Morii's entity decision);
synthetic validation.

## Context

Morii chose a combined approach for people, places and other entities in daily
memories (2026-11-23). An identical name links to an existing entity
automatically, marked as automatic and revocable. Everything else becomes a
suggestion, and he confirms it in the Runtime UI.

Daily memories had no entity records to link: the Runtime capture and ChatGPT
formation paths formed none (ADR-MEM-157/158), unlike initialization.

Entity records are part of a capture. A captured source cannot gain entities
afterwards, so they must be recorded with the turn they cite.

## Decision

This ADR gives daily memories their own entity records; the identity layer
follows in its own ADR.

- **Submission.** `ConnectorStatement` gains optional `entities` (at most 8).
  Each entry has a `name` (1–128 characters), an `entity_type`, a `reality` and
  a `role`. The remote write tool lists the field with its enumerations.
- **Capture.** `SegmentedAutomationVault::capture_runtime_with_entities(permit,
  request, entities, preflight)` records entities in the same capture as the new
  turns. `capture_runtime` calls it with none. Each `RuntimeCaptureEntity` names
  the newly captured user turn, a quote that resolves in it, a fresh
  `entity_id`, the type, reality and display name. The record follows the
  initialization rule (ADR-MEM-104 pipeline):
  - **One record per mention.** There is no name-based merge.
  - **Evidence.** The statement's quote.
  - **Sensitivity and access.** The turn's sensitivity and the window's access
    policy.
  - **Project.** The window's project for project entities.

  A missing turn, an assistant turn, an unresolved quote, or a duplicate or
  existing ID refuses the whole capture with `Conflict`. A replay of the same
  request keeps what its first capture recorded.
- **Formation.** `CaptureStatement` gains `entities: Vec<StatementEntity>`
  (`entity_id`, `role`). The formed memory mentions each entity with the
  statement's evidence.
- **IDs.** The connector derives each `entity_id` from the request ID and the
  statement and entity positions. A resubmission therefore names the records
  its first capture made, and a replay creates no duplicates.

Entity records stay bounded by their window's policy, as every entity is today.
Readers see a daily memory's entities exactly when they see the memory.

## Consequences

- ChatGPT can say who or what each memory is about. The catalog, Inspector and
  `same_entity` relations within a window show them.
- Linking these records to records in other windows (same name, Morii's
  confirmation) is the next slice. Until then they appear as separate entities.
- `CaptureStatement` has a new field. Analyzer implementations outside this
  repository add `entities: vec![]`.
- No persisted format change; records use the existing catalog shape.

## Verification

- **Connector.** A submission whose statement names a person and a place is
  formed with two mentions. Each record has the right name and type, and its
  evidence equals the memory's quote. Preflight writes nothing. Nine entities or
  a blank name are refused. Resubmitting replays with the same two records and
  no second memory.
- **Vault.** A missing turn, an unresolved quote, an assistant turn and a
  duplicate ID all refuse with nothing written. A valid capture records the
  entity with the window's access policy. A later capture cannot reuse the ID.
