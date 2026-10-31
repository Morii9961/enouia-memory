# MCP-3 captured formation worker

2026-10-31. Base 8a1c13d. [ADR-MEM-112](../adr/112-captured-formation-worker.md),
[contract](../contracts/CAPTURED_FORMATION_QUEUE_V1.md). Only newly allocated
synthetic roots and a scripted analyzer. No private root, Provider, embedding or
model call. Nothing here measures memory quality.

## Actual evidence

Ten cases in `crates/enouia-memory-init/tests/captured_formation.rs`:

- Formation: two captured turns become System memories in native turn order,
  with live mode, an exact quote and evaluator adoption. Reopen adds no analysis
  or commit.
- Exclusivity: a second worker gets Busy. Wrong actor, foreign journal and
  old-layout root refuse.
- Uncertainty: Paused keeps the job queued. Unknown, a panicking analyzer and a
  restart after dispatch all record `analysis_unknown`, never call analysis
  again, and let later turns continue.
- Untrusted analysis: an unresolvable quote is refused, an ambiguous statement
  stages automatically, and a job with nothing usable ends `rejected`.
- Evaluator refusal: a clear self-report quoting an assistant turn is refused by
  the support rule, recorded, and the next job forms.
- Moved head and policy gap: a commit between prepare and apply re-evaluates with
  no new analysis. A policy revoked and restored at a new revision resumes with
  zero refusals.
- Deletion after analysis forms nothing for that source; later turns still form.
- Revoked work waits with no analysis and resumes when the policy returns.
- Publication faults at StagingWritten and AfterCurrent report OutcomeUnknown and
  poison the worker. After recovery the chain decides, with exactly one receipt
  and one analysis call.
- Scheduling: captured work finishes before a pending backfill on one scheduler,
  then the backfill completes.

## Defects found and fixed

Self-review found two defects that the first eight cases passed over, and a new
case exposed a third. All evaluator results arrive as `AutomationFault::Contract`,
which the first version did not classify:

1. An evaluator refusal returned an error on every step and blocked the queue.
2. A commit between prepare and apply poisoned the worker on ordinary concurrency.
3. A prepared request naming a superseded policy revision got Denied at apply and
   was refused, which lost the statement after a routine policy update.

Prepare and apply are now separate steps, which also makes the concurrent path
testable without a timing race. Queue dispatch also moved from the exact enqueue
policy revision to the current one, after a first test showed that a routine
update stranded queued jobs.

`native_catalog_scope_pagination_fields_trace_lifecycle_and_business_bytes` failed
reproducibly at Vault(Denied) while the laptop ran on battery: it took about 137 s
against a 120 s real-time permit. It previously passed, and no code on its path
changed. Its scope now uses the 300 s maximum, with the expiry case moved to match.
No assertion was weakened. A separate control build of the prior commit was not run.

## Checks

Offline and locked, serial:

| Check | Tree | Result |
| --- | --- | --- |
| Workspace tests, no-fail-fast | Before the three-defect fix | 840 passed, 0 failed, 4 ignored |
| `enouia-memory-init` tests | Final | 27 passed, 0 failed |
| Fmt and strict workspace/all-target clippy | Final | Pass |
| Python 3.12 / jsonschema 4.26.0 | Before the fix; no schema change since | 63 schemas pass |
| Runtime surface | Final | Logged, Workspace IPC remains 58 commands |

The workspace run predates the fix. Later edits touch only `formation_worker.rs`,
its tests and documents, so the rerun covered the crates that use it. Two earlier
workspace attempts are not counted: one was stopped for a self-review edit and one
stopped at the timing failure above.

## Not proven

No real model, real concurrency between threads, or long-running behavior. The
stale path is exercised deterministically between steps, not under a live race.
Refusal by older binaries was not run. Nothing exercises a journal near capacity.

## Remaining work

The analyzer is scripted, and nothing starts the worker: there is no background
thread, foreground yield, cancellation, or Host wiring. Analysis sees one source
at a time, and statements cannot revise memories or link entities. A job whose
source is deleted after analysis stays `forming` with no effect. A lost analysis
leaves its source unformed until a later explicit re-analysis design. The Owner
Runtime wire, `conversation_capture`, `memory_note`, Core-4 continuous model
policy, Runtime adoption, private data and daily acceptance remain undelivered.
