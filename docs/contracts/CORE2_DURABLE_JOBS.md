# Core-2 durable jobs and resource isolation

Date: 2026-10-24. [ADR-MEM-86](../adr/086-durable-automation-jobs.md).
Base: Core-1 `678c2d75d0eb88f3c98c31a95d08a490d24627be`.

Core-3 amendment: optional native `spawn_with_completion` invokes a callback
after generation persistence, outside job control exclusion. Default `spawn`
retains generation-only behavior. The separate
[AutomationHost](CORE3_AUTOMATIC_LOOP.md) applies exact results to its selected
V2 root; the coordinator never creates a memory receipt by itself. Native
metadata-only generated bindings support recovery of an already committed
batch after invalidation, without granting new dispatch/application authority.

## Opt-in boundary and persistence

`enouia_memory_host::automation::SyntheticJobs` is an explicitly constructed
synthetic coordinator. `open` requires a verified isolated root and refuses a
root containing `vault`. There is no default root, production side database,
real Provider adapter, HostService/Workspace/MCP integration or automatic spawn.
It does not read CURRENT, Canonical or Activity/Usage and never acquires the
Vault writer lock. Core-3, not this coordinator, will own legal application.

Persistence is one bounded closed schema-2
[journal](../../contracts/next/job-journal-v2.schema.json) at
`sync-state/core2-jobs.json`. Reuse the existing managed path/reparse checks,
bounded read, flushed temporary sibling and write-through atomic replacement.
Hold a separate lifetime OS file lock at `sync-state/core2-jobs.lock`. Reopen
cannot overlap a live coordinator; the OS releases the lease after process
termination. The journal replaces all jobs, reservations, results and cursors
together. No new append protocol, database, dependency or CURRENT is introduced.

These are synthetic journal semantics, not a released V2 Vault. Existing record
schema 1 and layout 2, Host/MCP versions and Workspace's 55 commands are unchanged.
Unknown journal major, corrupt shape, unsafe counts and inconsistent state refuse
open without rewriting that journal. An existing Vault is rejected before any
job file is written. Root selection and native authentication remain host duties.

## Work identity, states and restart

Submit an ordered batch of 1–8 exact Core-1 `AdmissionInput`s with one current
`EvaluationContext` per input. All must pass the real Core-1 pure evaluator and
match one active native permit: snapshot, System actor and exact policy hash/
revision. Source revisions, target revision/hash, dependency selection, rule
provenance and content stay in the saved inputs. Mixed snapshots/policies/actors,
repeated operation/result identities and repeated logical items refuse admission.

The SHA-256 work key covers the ordered canonical inputs except generated
operation and result IDs. An identical resubmission returns the original job,
original operations and result identities. Changed work reusing an operation,
or an overlapping batch that would generate an existing logical item again,
conflicts. There is no pruning of dedupe history in this slice. Capacity refusal
does not create an owner review queue; future retention/budget policy must manage
bounded journals without silently losing unknown-operation history.

| State | Meaning / next behavior |
| --- | --- |
| queued | Cursor names a not-yet-dispatched item; eligible only under the current native permit and no foreground demand |
| running | Dispatch intent and reservation are durable; at most one outstanding item; outcome still uncertain |
| generated | All input-bound raw JSON responses and the final cursor are durable; no memory has been applied |
| budget_blocked | No additional dispatch; remaining work retained under the fixed ledger limits |
| invalidated | Lock/shutdown, expiry/regression or changed binding stopped the job; results and uncertainty retained |
| cancelled | Explicit job cancellation; no restart/resubmission can silently reactivate it |
| outcome_unknown | A dispatch may have happened without a durable usable result; never automatically resent |
| failed | Known refusal or invalid/over-budget output; no result application or automatic retry |

The cursor equals the count of saved exact results. For each item, persist
running intent and token/cost reservation **before** handing off generation;
persist its input-bound response and cursor advance together. Keep reservations even
after a known refusal or unknown charge: these are conservative budget units,
not actual account billing or model usage. The dispatcher never refunds them.
`Status.reserved_tokens` and `reserved_cost_microusd` describe the entire journal
ledger, not the individual job's usage or a Provider account balance.

Open starts locked. Running entries become outcome_unknown, with their original
IDs/reservations. Queued work resumes only after fresh explicit native activation;
saved results are never regenerated. Fresh `resume` reevaluates every input and
can restore only an invalidated job without an unaccounted invocation. It never
retries cancelled, failed or unknown work. A result-write failure that leaves an
old running intent recovers as unknown. A complete saved atomic result can be
reused; partial JSON is not interpreted as success. Any uncertain storage error
poisons this instance and stops new dispatch until reopen and inspection.

## Lifecycle and resource gate

`Permit::from_context` accepts the native trusted System/policy observation,
including policy validity. It does not authenticate that observation or grant
egress. The actual native adapter must call activate/lock/shutdown when current
Vault, primary, data/policy/deletion watermarks, policy, lock state or lifecycle
changes. This synthetic module has no background access to those real systems.
Every new dispatch compares its frozen binding to the current permit, rechecks
expiry/regression, persists intent/reservation and performs the bounded handoff
under the same gate as cancellation. Reopen never restores cached permission.

Lock/root/epoch/policy/deletion changes invalidate queued, running, generated and
budget-blocked jobs. In-flight invalidation/cancellation sets the stop token and
keeps uncertainty; late replies cannot restore state, append a result or advance
the cursor. Policy expiry and wall-clock regression also invalidate and stop
dispatch. Clock regression never lowers the saved watermark. `results` requires
a current matching permit and generated state; metadata status remains readable.

The coordinator owns one named private Memory thread. A second spawn is refused;
it never consumes Runtime's shared worker pool. Foreground demand prevents the
next dispatch, including the next batch item. Already in-flight work polls its
cooperative stop token and deadline outside all control and Vault writer locks.
The journal cap is 16 jobs, each up to 8 items; configurable limits cannot exceed
those caps. One in-flight generation, total reserved tokens/cost, per-call tokens/
cost, output bytes (up to 256 KiB), journal bytes (16 MiB) and per-call wall-clock
timeout (up to 60 seconds) are independently checked. No capacity/budget reset
on reopen is allowed; changed limits conflict.
Admission reserves worst-case future response storage (including JSON string
escaping and wrapper growth), so the journal cap cannot strand a completed
response at the cursor publication boundary. Cancellation releases this future
storage reservation while retaining the operation/dedupe history and charges.

`Generator::start` is a bounded **nonblocking handoff**, not the model wait.
`PendingGeneration::poll` must also be bounded and nonblocking. Both are tested
with fake implementations only. Panics are caught and recorded as unknown
without unwinding through the host or poisoning its control mutex. A pending
adapter never holds the Vault writer lock. Control status/cancel/shutdown do
not join a pending worker; shutdown first closes admission and sets cancellation.

A noncooperative poll can delay that private worker and deadline handling until
it returns, while status/cancel/shutdown remain usable. Rust cannot safely kill
such a thread. An untrusted or potentially noncooperative real engine needs its
own process boundary; this slice does not claim arbitrary adapter hard timeouts
or crash/abort isolation. The start contract must not be violated by a real
adapter, because its short handoff shares the cancellation gate.

## Generation is not application

Core-2 fake generation operates on an exactly selected Core-1 analysis request;
it does not implement semantic extraction or claim that a model understood
sources. The generator may produce a different interpretation, rather than
being required to echo a predetermined decision. Output carries the exact input
hash and an original JSON response within reserved resources and byte limits.
The coordinator computes the response hash and saves both hashes and original
response bytes. Reopening verifies the binding and byte hash, not semantic truth.
JSON validity is only a format check. A response claiming owner consent or a new
schema/permission remains untrusted data. No response is automatically parsed
as an admitted decision, passed to an existing writer or given approval power.
No ReviewRecord, owner confirmation, applied flag, Canonical change or successful
memory commit receipt exists here.

Core-3 must obtain fresh live source/target/policy/deletion/authority state,
parse the untrusted response, reevaluate its decision and exact binding (including
Core-1 knowledge/adoption and automatic staging), then apply decision and result atomically
through the legal single Host transaction. Only that transaction may produce a
memory success receipt. Provider continuous consent, credential handling and
actual network/embedding adapters remain Core-4 or separate explicit activation.

## Runtime consumption

This slice is library-only; Runtime keeps its old pin and embedded behavior.
Later native adoption must supply real lifecycle events and foreground demand,
own the private worker lifetime and keep status/progress/cancel outside stop/join
waits. Job progress must remain distinct from memory adoption and saving:
generated means saved untrusted generation response; running/unknown means no confirmed result;
invalidated/cancelled means no further dispatch; budget_blocked means resource
policy stopped it. Unknown or staged states must not become a mandatory owner
review backlog. No UI, IPC endpoint, notification or MCP tool is added now.

Evidence and remaining boundaries:
[Core-2 validation](../validation/CORE2-durable-jobs.md).
