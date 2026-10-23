# ADR-MEM-83 — Agent local current-memory context

Date: 2026-10-23. Status: accepted within explicitly authorized MV-8.2 local synthetic development.

Bind context_get to the existing deterministic compiler on the same Host Vault.
This slice supports the current commit, no session/branch, answer or
inspection_preview purpose, and the native fixed LocalMock destination only.
Respect project hints, asOf and the requested token ceiling. Refuse knownAt,
session/branch, other purposes, destinations or provider binding names before
compilation. Never ignore them, choose a fallback or activate a Provider.

A dedicated native compiler entry point accepts only an Agent principal and
sets ClientSurface::McpClient. Its durable requestId receipt namespace and
payload include that native surface/purpose; the existing CLI/Workspace compiler
paths and their receipt payloads stay unchanged. A context request may persist
its capsule/inspection even though it does not create a Canonical memory.

The native ContextRead ceiling does not expose the memory/source read tools.
Existing current persisted per-resource compiler policies still apply, including
the index's memory-read filter and independent ContextRead decisions. An empty
or policy-limited result must retain its incomplete/no-evidence metadata; never
invent context or rewrite query text into approved user evidence. Inspection
details are not returned, only its opaque ID and the validated capsule.

Append a content-free admission audit before compilation and a final audit before
delivery. Check current native authority, source/provenance/policy/deletion
freshness and the compiled request/principal/surface/purpose/destination. A
potentially committed storage or post-compilation audit failure is outcome_unknown
with the original packet/requestId (write key remains null). End the session and
never automatically recompile. Explicit new-session original-requestId replay
can recover the original capsule, subject to current barriers.

Historical/session context and session_checkpoint binding remain unfinished.
This is an offline synthetic compiler boundary, not external Agent authentication,
real-client compatibility, Provider egress approval, Runtime adoption, cloud,
private-data consent or personal acceptance.
