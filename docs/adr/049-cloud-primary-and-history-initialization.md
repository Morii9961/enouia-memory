# ADR-MEM-49 — Cloud primary and historical initialization

Date: 2026-10-11. Status: **Adopted direction; H1 offline planning implemented.** Cloud, MCP, deployment, migration and automatic acceptance are not activated.

## Context

The owner requested complete memory access while the PC is off, initialization from the received ChatGPT history, and a recall/proposal cycle through MCP. The owner then asked to continue along that direction. The earlier local-primary/gateway-only target cannot satisfy the availability requirement.

## Decision

- Target one cloud Memory primary; local initialization, offline copies and independent encrypted recovery remain. Runtime owns the local product frontend. Memory owns domain, cloud service, adapters and contracts.
- Preserve raw evidence, reviewed Canonical, candidates, sessions, rebuildable indexes, valid/known time and deletion barriers. Full service-side computation places the service in the trust boundary; each client still has limited access.
- Initialize history in stages: archive and reconcile, plan bounded branch-preserving work, extract candidates, reconcile changes over time, owner review, derived topic navigation. Source coverage is distinct from semantic completeness.
- H1 adds a pure read-only planner over existing parsed adapter output and a built-in synthetic example. No new persistence, wire contract, credentials, model calls or automatic owner approval.
- Automatic acceptance remains deferred. Archive/expiry is distinct from owner-authorized permanent deletion. MCP receives only explicitly delivered data, not an assumed full third-party conversation.
- MV-8 freezes the single-host/platform/primary-fencing contracts; MV-9 implements and validates the cloud primary; MV-10 implements local copies/offline proposals. A stage still starts only on explicit owner instruction.

## Relationship to prior decisions

ADR-MEM-01's local-primary **target** is superseded; the existing local store remains the implementation. ADR-MEM-11/27's single Host is retained and extended to the cloud target. ADR-MEM-15/29's gateway-only target is superseded by the MV-9 cloud service. ADR-MEM-16/17's explicit trust, single primary and proposal-only offline edits remain, with the primary eventually moving to the cloud. ADR-MEM-45's current embedded Runtime adapter stays until a separately versioned migration. ADR-MEM-47/48 Provider and workspace semantics are unchanged.

## Consequences and gates

There is no claim of Linux support from Windows root/ACL tests. Cloud port selection and authentication require independent evidence. Switching primary requires a verified snapshot, deletion/policy reconciliation, independent encrypted recovery, owner confirmation and fencing of the old writer. No file synchronization of a live Vault and no dual-primary fallback.

Real materials, calls, hosting resources, trust scope, backup/recovery secrets, client selection and automatic-acceptance policy remain explicit activation decisions. Existing manual project status snapshots are not updated during development.

Normative details and acceptance exits: [design package v1.3](../design/CLOUD_PRIMARY_AND_HISTORY.md). H1 evidence: [validation](../validation/MV-7-history-planning.md).
