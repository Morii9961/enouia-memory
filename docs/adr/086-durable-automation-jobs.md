# ADR-MEM-86 — Durable isolated automation jobs

Date: 2026-10-24. Status: Adopted, Core-2 synthetic implementation only.
Morii's instruction to continue the next step authorizes Core-2 after Core-1.
It does not authorize Core-3, real Provider sending, production side databases,
Runtime adoption or changing existing Host/MCP/owner paths.

## Decision

Add an opt-in `host::automation` coordinator for explicitly chosen, verified,
isolated synthetic roots. Refuse roots containing an existing Vault. Persist a
bounded schema-2 job journal in sync-state using existing ManagedRoot atomic
replacement and a dedicated lifetime OS lock. This is job state, not Canonical
authority, a V2 Vault writer or a second production database. No default entry
point instantiates it. The complete job journal is the single atomic boundary:
inputs, reservations, invocation state, result and cursor move together.

Admit exact Core-1 inputs through current trusted evaluation contexts. Deduplicate
the ordered work content independently of generated operation/result IDs; return
the original job and identities. Reusing an operation for changed work conflicts.
Keep the exact snapshot, policy hash/revision and source/target revisions. The
native lifecycle permit is a trusted observation, not authentication or an
egress grant. Policy expiry, clock regression, lock/root/primary/policy/deletion
change, cancellation and shutdown prevent new dispatch. Reopening starts locked;
resumption needs fresh explicit native admission.

Use one private Memory worker, never Runtime's shared pool. Foreground demand
prevents further background dispatch. The Generator start operation is a bounded
nonblocking handoff under the same control gate as cancellation; waiting and
polling occur outside all coordinator and Vault writer locks. Generator adapters
must obey this nonblocking contract and perform cooperative bounded polling.
No real adapter is added here. A stuck or panicking adapter cannot take down the
calling process through unwinding; cancellation/status/shutdown do not join it.
Noncooperative code cannot be forcibly killed safely inside Rust; subprocess
isolation remains required for untrusted adapters.

Persist a conservative token/cost reservation before dispatch, never refund an
unknown charge. Bounded capacity, one in-flight call, output bytes and deadlines
are independent limits. Persisted dispatch without a completed saved result
becomes outcome_unknown on reopen and is never automatically resent. Cancellation
during a dispatch keeps uncertainty visible; late replies cannot publish a
result after lifecycle invalidation. A persistence error poisons this instance,
retains the original operation and requires reopen/inspection rather than retry.

Each batch cursor advances only with one response bound to its exact input in
the same atomic write. Preserve original response JSON, input hash and response
hash; responses are untrusted data, not preapproved decisions. Saved results
survive reopen without regeneration. Generation completion
is not application or a successful memory commit: Core-2 offers no applied state,
Canonical write, ReviewRecord or receipt. Core-3 must revalidate dependencies and
current authority inside its legal transaction before applying a parsed result.

## Evidence and later adoption

The schema and native validator describe the same closed journal; independent
schema checks cover structure, native tests cover lifecycle and concurrency.
See [Core-2 validation](../validation/CORE2-durable-jobs.md). Process-crash tests
do not prove power-loss durability, OS authentication, real model quality,
continuous egress consent or Runtime product acceptance. Future Host/Runtime
integration must bind these lifecycle calls to actual native events; this
synthetic module does not observe them automatically.
