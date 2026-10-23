# ADR-MEM-81 — Host-scoped Agent reads

Date: 2026-10-23. Status: accepted for the explicitly authorized MV-8.2 local synthetic continuation.

The first domain binding implements memory_search, memory_read and memory_source
inside the existing HostService, borrowing its already bound Vault. It opens no
second Vault or writer and never forwards to the owner Workspace. A trusted
embedding adapter must separately authenticate/enroll an Agent before supplying
ADR-MEM-80's Grant. This library admission is not OS authentication, a pipe
extension, real-client registration or a deployable stdio server.

Only memory:read and source:read ceilings are accepted by this binding; its MCP
catalog therefore exposes only the corresponding read tools (search/read for
memory:read, source for source:read). Other
scopes are refused at construction. Tool packets cannot choose the principal,
scope, Vault, root, connection or lifecycle action.

Agent connections share the Host lifecycle gate's instance, expiry and capacity.
They are a distinct connection kind: an owner/window handle cannot dispatch an
Agent call, and an Agent handle cannot dispatch Workspace calls. Lock, unlock,
revoke_all and clock faults invalidate them. A new unlock does not revive old
connections. Synchronous reads finish under the same gate used by owner calls;
native lock waits for that boundary before returning.

Read authorization uses current persisted PolicyRecords, independent scopes,
the native principal and server-resolved project/sensitivity metadata. Genesis
owner-only policy grants Agents nothing. Historical record reads must satisfy
both current and requested revision policy limits. Sources have no project
field, so a project-specific policy does not authorize a source by guessing its
project from client arguments. Related memories and attachment metadata are
filtered independently. Missing and unauthorized direct targets return the same
non-disclosing not_found code.

Reuse the existing permission-filtered index and locator resolver. Current
tombstones, purged records, broken provenance, byte bounds and UTF-8 boundaries
are checked before delivery. Recheck authority and policy/deletion freshness;
never send stale output or silently rerun a request. Append an existing typed,
content-free audit event before returning external read results. Audit failure
fails closed. No query, source text, path, key or raw exception enters the audit.
Raw source objects are capped at 64 MiB before allocation; result JSON is capped
at 128 KiB, with the existing 8 KiB source excerpt limit. Unsupported/budgeted
reads fail explicitly instead of replacing bytes or silently truncating records.

Proposal, checkpoint and context domain binding remain separate slices. This
decision does not enable Provider send, owner review, private data, Runtime UI,
MCP installation, cloud, migration, merging or deployment. Synthetic Host/domain
evidence is distinct from authenticated external-process and personal acceptance.
