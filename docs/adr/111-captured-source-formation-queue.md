# ADR-MEM-111 — Queue-only admission of captured Runtime sources

2026-10-31. MCP-3 continuation of ADR102, consuming the shared-queue interface
proposed by the initialization session and released to the MCP session.

Captured daily sources need a durable wait for System formation. They must not
be recaptured as imports, and Host must not depend on the initializer crate
(which already depends on Host). The queue therefore lives in the Vault layer,
on the same leased Vault instance and writer.

## Decision

Add `SegmentedDelta::Enqueue`, accepted only on automation-segmented-owner-v1
and only with the System actor. `SegmentedAutomationVault::enqueue_captured`
takes the System actor, a NativeRuntimeCapture and a PolicyRef. It returns a
CapturedQueueReceipt with a stable queue_key, job_keys and state `queued`.

The capture value is data. Admission requires the Owner capability, a real
non-preflight capture and this Vault, then re-reads every binding from state:
exact current source revision, RuntimeEvent kind, runtime.capture provenance
(request id, request hash, producer), session, current active policy, source
pool, sensitivity and deletion. Sources must be in strict native turn order.
The check runs inside State::check, so chain replay refuses a forged commit too.

Identity is the original capture key: vault, producer and request id. Policy is
deliberately excluded, so a new policy cannot mint a new key. Repeating the same
binding returns the original receipt with no commit. A changed request hash,
source set or policy under that key conflicts. Nothing rebases.

Capture and enqueue are separate transactions. A crash between them leaves a
captured source that the producer's replay can enqueue under the same key.
Enqueue sends nothing, so an uncertain publication is resolved by repeating the
same key after recovery. No alternative key exists. NativeRuntimeCapture keeps
`formation_scheduled=false`: that receipt reports source publication only.

`queued_captures` is the System read port for a later worker. Sessions keep
first-enqueue order and each follows native turn position, not message time or a
random ID, so a late enqueue after a crash still precedes later turns. Deleted
sources drop out. A job outside current policy holds the rest of its session
rather than being overtaken. Enqueue records change no memory or source, so
authorized process views do not reset.

## Compatibility

| Consumer | Behavior |
| --- | --- |
| segmented-host-v1 | Refuses Enqueue before identity checks. Layout unchanged |
| Owner root with no Enqueue commit | Unchanged, including for older binaries |
| Owner root with an Enqueue commit | Older binaries refuse the unknown delta when loading the chain, before business reads; the root stays on versions with this ADR |
| Workspace IPC, Memory IPC, MCP pipes | Unchanged; no new command, tool or wire |
| Rollback | Only to a version with this ADR once an Enqueue commit exists |

## Not delivered

No formation worker, dequeue or completion state, generation, Provider call or
job result. A queued entry is never reported as formed. Live priority over
backfill awaits the worker, because backfill work is still in the initializer's
journal. The Owner Runtime wire, Agent hosts, `conversation_capture`,
`memory_note`, Runtime hook/config/pin/UI, private roots and real models remain
separate. See [contract](../contracts/CAPTURED_FORMATION_QUEUE_V1.md) and
[validation](../validation/MCP-3-captured-formation-queue.md).
