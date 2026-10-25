# Core-4 continuous policy, synthetic execution

Date: 2026-10-24. [ADR-MEM-88](../adr/088-continuous-generation-policy.md).
Base: Core-3 `564fc7d5f493404e1a3746e7e2ff744a4bd2ffb2`.

`next::ContinuationPolicy` is separate from memory admission and V1 egress
approval. It delegates one fixed purpose: generation of an AdmissionInput-based
memory analysis. The native owner grants exact System, Vault, subject/project,
admission policy revision/hash, producer, source IDs, destination API/model/
adapter version, sensitivity ceiling, validity and per-call/total token/cost
ceilings. Highly sensitive content is refused. API identifiers describe a future
binding; they do not construct endpoints or authorize arbitrary URLs.

`automation::continuation::ContinuousPolicyHost` opens only alongside its exact
SyntheticJobs instance/root. It reuses managed bounded atomic files and a lifetime
OS lease at `sync-state/core4-continuation.lock`. A closed schema-2 journal at
`sync-state/core4-continuation.json` retains native binding, full policy revision
history and up to 64 content-free attempts. The cap is 1 MiB; charges are shared
across revisions and never reset by reopening, locking or renewing a policy.
It is job accounting, not memory authority or a production side database.

Open starts locked. An independently authenticated native context activates an
exact current admission permit. `set_policy` requires the bound Owner and the
same policy identity at revision +1; revocation is an inactive next revision.
Policy changes/lock invalidate associated Core-2 jobs before changing the grant.
Native root/policy/deletion changes still use Core-3's existing job invalidation.
No model field, source instruction, MCP grant or page packet supplies authority.

`fake_generator` accepts only a synthetic destination, with no HTTP or secret
implementation. The native host binds the controller using `with_continuation`
before starting its worker. The generator preflight pauses queued work without
spending or modifying inputs if locked, absent/expired/out-of-scope, already
attempted, busy or over budget. `paused` exposes the native reason; the durable
job remains queued. Setup/renewal is optional owner control, not a per-memory
review queue. A native grant refresh requires fresh activation/submission of a
safe invalidated job; it never fabricates a private-send confirmation.

At actual start, recheck policy under a shared native gate, durably reserve one
attempt, then perform a bounded nonblocking fake handoff. One attempt may be in
flight. Revoke cannot return before an already admitted handoff crosses that
gate. No waits or unbounded computations belong inside it. Output polling checks
stop, epoch, expiry and grant. Dropped pending work becomes unknown; reopen
converts Running to outcome_unknown and never resends it.

The only Core-2 reservation reversal is `Generation::Paused` returned directly
before transport handoff: it preserves queued work and reverses that known-unsent
local intent. A crash before reversal remains unknown. After handoff, a transport
Paused result is converted to Unknown. No continuous-policy attempt is refunded.
Unknown/failed calls retain reservation floors; known usage charges the maximum
of reserved and observed values. Overruns prevent later use of exhausted budget
and are not adopted. Invalid/unrepresentable accounting or uncertain persistence
fences the controller. Actual token/cost bounds remain adapter obligations.

Application shares a policy read barrier held through Core-3 publication;
revocation holds its write side. Every selected input/response/operation must
have an exact completed attempt under the current policy and native snapshot.
Old generation cannot cross a later withdrawal into a memory transaction.
Already committed content-free receipts may still be recovered under Core-3's
current policy/deletion checks: receipt recovery creates no new send or memory.

| Consumer/version | Behavior |
| --- | --- |
| V1 Provider/Session/Approval | Unchanged: private requests still require the existing exact approval |
| Default Core-2 generator | `ready` defaults true; generation-only behavior unchanged |
| Core-4 opt-in fake generator | Standing scoped policy, durable accounting, automatic pause; no request approval records |
| Old next consumer | No continuation dispatch; unsupported kind fails before interpretation |
| Runtime/MCP/Workspace | No route, tool, command or pin change; later capability/native lifecycle adoption required |

Disabling this controller leaves audit/accounting intact. Do not delete the
journal to reset budgets or replay attempts; do not downgrade or transplant it
as a new authority. There is no production migration or independent restore
tool here. Real Provider adapters/account activation, prices/tokenizers,
credentials, live remote cancellation/billing and Runtime UI remain outside
this slice. See [actual checks](../validation/CORE4-continuous-policy.md).
