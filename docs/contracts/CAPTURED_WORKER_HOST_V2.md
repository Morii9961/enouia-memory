# Hosted captured worker / native analysis v2

2026-11-01. ADR-MEM-114. Native-only captured-analysis-native-2.

CaptureAnalyzer::start(job, source, stop) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis>.
PendingCaptureAnalysis::poll(stop) -> Option<CaptureAnalysis>.
Both calls are bounded and nonblocking; long work belongs to an adapter's
cancelable asynchronous operation. StopToken is one-way, clonable and not authority.
Paused proves no handoff. Unknown, timeout, cancellation and panic never retry.
An immediate result may be returned from start; it is subject to the same bounds.

CapturedFormation::open uses a 30-second analysis deadline; open_with_timeout
accepts a positive duration up to 300 seconds. step_with_stop drives one bounded
action. Pending polls can return false, and has_pending prevents initializer
backfill from overtaking them. At most one analysis is in flight per worker.
Output is at most 32 statements and 256 KiB; oversized output is rejected.

RuntimeCaptureHost::start_worker(analyzer, timeout) returns a native handle.
Hold it for the unlocked native session; dropping it stops and joins. Native
set_foreground(true/false) pauses background work; capture/preflight/enqueue/status
automatically announce foreground priority. Native revoke permanently cancels
this enrollment. Peer death/expiry, policy changes, drop and worker storage
uncertainty also close it. Native shutdown is bounded by the bounded adapter
calls and one local Vault step. No arbitrary blocking adapter is supported.

| Case | Result |
| --- | --- |
| Owner native + Owner root + live retained Runtime peer | Dedicated automatic System worker |
| Old layout/profile, second worker on the root | Refuse / Busy |
| Foreground active | No new background step; queued/pending work waits |
| False/Busy/pending | Bounded sleep/backoff, no spin |
| Lock/revoke/exit/expiry/policy withdrawal | Stop token, no later publication/dispatch |
| Analysis unknown/panic/timeout | Terminal analysis_unknown, later turns continue |
| Publication OutcomeUnknown | Worker and enrollment stop; recover and freshly enroll |
| New native source + old synchronous analyzer | Compile-time refusal; explicit adapter update |
| Existing pipe-1/pipe-2 and stored journal v1 | No wire/storage migration |

Only synthetic roots, owned processes and fake analyzers are exercised. No real
Provider/model/embedding, Runtime pin/config/hook, Agent hook, memory_note or
mandatory human review workflow. Native callers must adopt the new analyzer API
and lifecycle controls; no Runtime repository is changed here.

2026-11-03: [analysis native v3](CAPTURE_ANALYSIS_V3.md) extends start with an
explicit bounded saved-hint slice. Adapters must update that signature; the
worker hosting/lifecycle contract here otherwise retains its meaning.
