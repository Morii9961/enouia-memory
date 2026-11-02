# ADR-MEM-114 — Hosted captured formation worker

2026-11-01. MCP-3 slice B, based on ADR-MEM-112/113.

Native Owner setup explicitly starts one Memory-owned background thread on the
same leased Vault. It uses the existing worker lease; no second writer, root or
Runtime worker-pool task. Foreground native calls announce priority before the
control gate. A productive step yields; false/Busy and pending polls back off
from 10 to 250 ms, with a condition variable for native wake/foreground controls.

Replace synchronous CaptureAnalyzer::analyze with captured-analysis-native-2:
bounded nonblocking start plus PendingCaptureAnalysis::poll and a shared stop
token, following Core-2 without a Vault-to-Host dependency. Default deadline is
30 seconds, configurable up to 300 seconds. Results are bounded to 32 statements
and 256 KiB. Start/poll never perform synchronous model/network/embedding waits.
Only fake native adapters are delivered; real model/policy/budget/egress choices
remain the owner's decision. There is no compatibility bridge to synchronous
analyzers: native implementations must update explicitly.

Dispatch intent is durable before handoff. Pending analysis is polled outside
Vault locks; no poll/restart resends. Cancellation, deadline, panic or lost
analysis becomes analysis_unknown and never gets resent. Only proven Paused
(no handoff) returns to queued. Unknown analysis releases later turns, without
creating a human review queue. Pending live work holds back initializer backfill.

Host revoke/lock/shutdown, peer exit/expiry and current policy withdrawal stop
the worker and cancel in-flight analysis. Native lifecycle and bounded steps
share a gate: after stop returns there is no new dispatch/application. The stop
token is signaled before waiting for that gate. Dropping the handle stops and
joins the dedicated thread. Panic is contained; storage OutcomeUnknown stops
the entire worker and enrollment until recovery and fresh native setup. No
automatic reconnect/retry/replacement key exists. Journal recovery remains v1;
unfinished dispatches become analysis_unknown on reopen.

Wire pipe-2, storage deltas and old capture/Agent profiles are unchanged. The new
native analyzer API is an explicit source compatibility break only. The existing
single-source/no-entities/no-revisions/deleted-forming and journal-capacity limits
remain. See [contract](../contracts/CAPTURED_WORKER_HOST_V2.md).
