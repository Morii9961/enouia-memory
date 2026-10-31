# ADR-MEM-112 — Shared captured formation worker

2026-10-31. MCP-3 continuation of [ADR-MEM-111](111-captured-source-formation-queue.md).

Queued captures need one System route to real memories, with the same rules as
initialization. Runtime producers supply raw turns only. Analysis is the
uncertain step: it may reach a model, cost money and lose its reply.

## Decision

`CapturedFormation` in the Vault layer runs one bounded step at a time on the
caller's leased Vault instance. A separate lease allows one worker per root. It
opens no second Vault and no second writer.

Each step takes the first dispatchable job from `queued_captures`:

1. Persist the dispatch intent, then call the native `CaptureAnalyzer` outside
   every Vault lock.
2. Persist the analysis as seeds before any evaluation.
3. Prepare through `prepare_formation` and persist the exact request, response
   and binding.
4. Apply through the existing `apply_batch`.

Analysis output is untrusted. Quotes must resolve exactly in the job's source,
and the existing evaluator decides adoption. Ambiguous or inferred statements
stage automatically, and no statement waits for a person. An unresolvable quote,
a refused evaluation or a suppressed claim is counted as refused and recorded,
never repaired, and the queue moves on. Evaluator rules are told apart by their
stable IDs: `next.snapshot_stale`, `next.policy_inactive` and
`next.policy_binding` say nothing about the analysis, so they re-evaluate instead.

`formation_seed`, `quoted_evidence` and `prepare_formation` are shared with the
initializer, so live and backfill records have one shape and one route.

## Outcomes

| Event | Result |
| --- | --- |
| Paused: proven no handoff | Job stays queued, no state kept |
| Unknown, panic, or restart after dispatch | `analysis_unknown`, terminal, never resent |
| No memory, or empty statements | `no_memory`, terminal |
| More than 32 statements, or every statement refused | `rejected`, terminal |
| At least one receipt | `formed`, terminal, receipts recorded |
| Crash while applying | Chain decides: receipt recorded, or proven not admitted and re-evaluated from durable seeds with no model call |
| Moved head or policy gap | Re-evaluated from durable seeds; a policy gap also holds the job |
| Publication unknown | Worker poisoned; reopen after Vault recovery |

An uncertain analysis releases its session instead of holding it. Holding would
stop all later turns of that conversation behind one lost reply, and resending
is forbidden. The source remains stored, and any later re-analysis must be a
separate explicit design. This is the deliberate cost of never resending.

A proven non-admission may re-evaluate because the chain is authoritative under
the worker lease after recovery, and nothing leaves the machine. The operation
identity may change; the job key and analysis do not.

Progress lives in the bound journal `sync-state/captured-formation-1.json`, as in
Core-2 and the initializer, because a chain commit between prepare and apply
would move the head. Jobs keep only receipts and counts once terminal.

Queue dispatch follows each policy's current revision. Binding jobs to the exact
enqueue revision would strand them after any routine policy update. The stored
original binding still decides replay identity under ADR-MEM-111.

## Scheduling

`Initializer::step_with_captures` runs captured work before any plan, so live
always wins over backfill on one scheduler. It depends on the Vault layer only,
which keeps Host from depending on the initializer crate.

## Not delivered

The analyzer here is scripted: no real model, Provider, prompt, embedding or
continuous policy. Nothing starts the worker. There is no background thread,
foreground yield or Host wiring, and a real analyzer needs bounded polling and
cancellation first. Analysis sees one source with no conversation context.
Statements cannot revise a memory or link entities, because catalog publication
for daily sources does not exist yet. A job whose source is deleted after
analysis forms nothing and stays `forming`. Agent capture tools, the Owner
Runtime wire, Runtime adoption and quality evaluation remain separate. See
[contract](../contracts/CAPTURED_FORMATION_QUEUE_V1.md) and
[validation](../validation/MCP-3-captured-formation-worker.md).
