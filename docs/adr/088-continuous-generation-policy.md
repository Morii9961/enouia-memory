# ADR-MEM-88 — Bounded continuous generation policy (Core-4)

Date: 2026-10-24. Status: Adopted for isolated synthetic execution.
Morii explicitly authorized Core-4/5 after Core-3; real accounts, private data,
Runtime adoption and deployment retain separate resource boundaries.

Add a closed next-version continuous-generation policy, distinct from memory
admission and V1 Provider approval. Native owner setup grants exact System,
Vault, admission policy revision/hash, subject/project, source IDs, producer,
destination API/model/adapter, sensitivity ceiling, validity and per-call/total
token/cost budgets. No per-request confirmation is fabricated. Highly sensitive
data remains forbidden. Credentials remain outside these types and journals.

Use a bounded auxiliary journal in the explicitly selected synthetic job root,
with managed atomic replacement and an independent lifetime OS lease. Reserve
before a nonblocking fake transport handoff. Keep original input/work identity,
policy binding and conservative accounting across revisions and restart. Unknown
outcomes never resend or refund; observed overruns increase charges and prevent
further budget use. Reopen starts locked and converts incomplete sends to unknown.

An opt-in generator gate pauses durable queued jobs without sending when no
policy is usable. A known pre-handoff policy race may return Paused: Core-2
undoes only its local unsent reservation. After any transport handoff, Paused is
forbidden and uncertainty retains reservations. Revocation/lifecycle/expiry
serialize with handoff and invalidate in-flight output. The fake adapter has no
credential or HTTP implementation; real API bindings are design identifiers,
not activated endpoints. Existing V1 Provider private-confirmation semantics,
default Host/MCP/Workspace routes and Runtime pins are unchanged.

See [contract](../contracts/CORE4_CONTINUOUS_POLICY.md) and
[evidence](../validation/CORE4-continuous-policy.md). Synthetic transport and
structural gates do not prove live billing, token bounds or remote cancellation.
