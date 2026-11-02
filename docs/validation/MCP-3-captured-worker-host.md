# MCP-3 hosted captured formation — slice B

2026-11-01. Base e6becf6. ADR-MEM-114 / CAPTURED_WORKER_HOST_V2.
Only new temporary synthetic roots, owned child processes and fake analysis.
No private Vault, real model/Provider/embedding, credentials, Runtime edit or
client configuration. No semantic quality or daily-use acceptance is claimed.

## Delivered behavior

A native RuntimeCaptureHost starts and owns a dedicated Memory formation thread
on the same leased Owner Vault. The existing worker lease prevents a second
worker/writer. Native foreground calls announce priority before taking the short
control gate; explicit foreground mode pauses background steps. Productive steps
yield, and false/Busy/pending waits back off with a condition variable.

Native analysis is captured-analysis-native-2: nonblocking start and bounded poll,
a shared one-way stop token, default 30-second deadline (positive, <=300 seconds),
at most 32 statements and 256 KiB of serialized output. There is no synchronous
analyze compatibility bridge or real model call. Vault defines the port without
depending on Host. The initializer does not overtake pending live analysis when
poll returns false.

Native revoke/lock/withdrawal/exit, current policy change, retained peer death or
expiry stop execution. Stop signals cancellation before waiting for the current
bounded step; no dispatch/application follows a completed stop. Worker-handle
Drop joins the dedicated thread; final Host reference release avoids taking its
own lifecycle gate recursively. Analysis panic/timeout/unknown becomes terminal
analysis_unknown and releases later turns, with no automatic resend or human
review queue. Storage OutcomeUnknown stops the worker and enrollment until
Vault recovery and fresh native setup. Original packets/keys are preserved.

## Version and refusal matrix

| Surface | Behavior |
| --- | --- |
| Owner layout + explicit Owner Runtime enrollment | Native automatic worker and existing pipe-2 |
| Old root/capability, wrong actor, second worker | Refused / Busy, existing lease retained |
| Synchronous CaptureAnalyzer implementation | Must explicitly adopt start/poll; compile-time break |
| Foreground active, empty/Busy/pending work | Yield/backoff, no empty loop |
| Revoked/expired/dead peer/current policy change | Stop and cancel; fresh native setup required |
| Lost analysis/panic/deadline | analysis_unknown, no second handoff; later turns continue |
| Storage OutcomeUnknown | Worker + enrollment close; chain recovery decides the receipt |
| Pipe-1/pipe-2 schemas, capture request keys, journal v1 | Unchanged; no migration or replacement key |
| Runtime/Agent/Workspace public tools | No new tool, scope or shared-shell change |

## Actual focused evidence

Final offline/locked run: **16 Host capture/worker tests, 6 Owner wire tests and
12 captured formation/initializer tests passed**, with no failures. Final fmt,
strict workspace/all-target Clippy and Runtime surface check passed before the
final workspace run. The same source is used by that run.

- Automatic formation: a dedicated thread finishes two captured turns, including
  a pending poll that later completes. Foreground mode blocks dispatch, then
  allows progress. Two actual memory receipts exist, and stopping frees the lease.
- Five separate lifecycle cases retain the Host while revoke, expiry, policy
  withdrawal or process exit causes stop; the fifth releases the Host itself.
  In-flight fake tokens cancel, no memory forms, reopen marks analysis_unknown,
  and analysis is called exactly once. Pending polls are bounded instead of spinning.
- Both start and poll panic are contained. Unknown/panicked first turns do not
  block a later turn; the Host remains usable and the first analysis never repeats.
- A never-completing pending analysis times out, cancels and stays terminal.
- StagingWritten and AfterCurrent publication faults stop the thread/enrollment.
  After recovery one genuine receipt exists and no second analysis occurs.
- A lost capture response revokes concurrently with the worker, retains the exact
  original packet and keeps the original OutcomeUnknown classification.
- A real owned Runtime pipe sends capture, gets distinct capture/queue receipts,
  then polls its authorized status until a background System commit exists.
  Status reports one genuine receipt with no source text or memory IDs and
  validates against the frozen pipe schema.
- A barrier releases policy update and prepared apply on different threads,
  repeated across three fresh roots. Chain truth yields one receipt and one
  analysis, with no duplicate memory. Existing stale-head tests remain intact.
- Oversized statement counts and serialized output are rejected without memory
  publication. Pending live analysis returning false does not start backfill.

Final offline/locked serial workspace run, --no-fail-fast: **861 passed,
0 failed, 4 ignored**, across 104 test/doc-test targets. The run exited 0
and uses the final source without later implementation edits. This also covers
slice A's final equivalent syntax/test-import adjustments. Fmt, strict
workspace/all-target Clippy and surface checks passed on that same source.
Aggregate: `883371a8e735a6e561042e47d82fa9d61d7eaa8d96b51235cf117295a3d6a60f`. Local ignored logs:
target/mcp-segmented-validation/captured-worker-b-*.log. No schema or validator
changed in B; the independent 64-schema cross-check from A is reused, not rerun.
Runtime surface is regenerated in this commit; Workspace IPC remains 58 commands.

## Failed and interrupted attempts

The first focused run passed the ten formation tests and nine of ten Host tests.
One Host test caught foreground-control closure masking OutcomeUnknown as Locked.
The native liveness/unknown check now precedes foreground admission, preserving
that contract; the final rerun passed. A later test-only compile failed because
new Host fakes referenced initializer-only helper functions and one match arm
returned a current pointer instead of unit. Replaced those test constructors and
fixed the arm; no Host-to-initializer dependency was introduced.

Self-review fixed two concrete lock-order issues before final acceptance: the
last Host reference could be released while holding the lifecycle gate, and
response_lost retained its packet mutex while waiting for that gate in the
opposite order to worker liveness checks. Guards/references now release in a
consistent order. Lifecycle/drop and concurrent lost-response tests pass.
The first workspace attempt was stopped during compilation for the packet-lock
fix and is not counted as a test pass. The final full run starts after that fix.

## Not delivered or proven

Real analyzer/model selection, continuous Owner model policy, budgets and egress
remain Morii decisions. This slice does not activate them. Adapters must honor
bounded nonblocking start/poll/drop; arbitrary blocking native adapters are not
supported. Real semantic quality, private data, Runtime adoption/pin/hook/UI,
external Agent capture, memory_note, MCP-4/5, merge and deployment are not delivered.
Existing single-source context, no daily entity/revision publication, deleted
sources potentially remaining forming, journal-capacity limits, long-duration
soak and old-binary rollback execution remain unproven/unresolved.
