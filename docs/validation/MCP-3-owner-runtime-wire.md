# MCP-3 Owner Runtime wire — slice A

2026-11-01. Base 7b6c424. ADR-MEM-113 and OWNER_RUNTIME_CAPTURE_PIPE_V2.
New synthetic temporary roots and owned child processes only. No private Vault,
real analyzer, Provider, embedding, Runtime edit or client configuration.

## Behavior and versions

memory-runtime-capture-pipe-2 accepts only automation-segmented-owner-v1 and
memory-runtime-owner-capture-native-1. Pipe-1 and all Agent profiles are unchanged.
Preflight publishes nothing and queues nothing. Write returns separate genuine
capture and queue receipts from separate transactions. Queue success is never
reported as memory formation. A capture/enqueue crash gap is repaired only by
explicit original-key replay. Unknown preserves the exact packet and closes;
neither client nor server reconnects, resends or changes keys automatically.

Status reads only the enrolled producer's exact original request and queue.
Current native permit, policy, source scope and deletion authorize before the
journal read. It exposes job keys, state and counts, with no text or memory IDs.
The journal can be observed without borrowing the worker lease or opening another
Vault. The new queued projection is never persisted as a new journal state.

| Input | Result |
| --- | --- |
| Owner native + Owner root + pipe-2 | Explicit capture/queue/own status |
| Owner native + pipe-1 | Refused before bind/body, unchanged |
| Old native or Agent capability + pipe-2 | Refused before bind/body |
| Wrong retained kernel peer, old hello/profile | Refused before transaction |
| Revoked/expired/dead peer | Closed; no new publication |
| Changed text/hash under same request | Conflict; original receipt unchanged |
| Foreign producer/request or inactive policy | No status disclosure |
| Lost reply after committed capture and queue | Exact call retained, client closed |

## Actual checks

Focused evidence: 10 Owner capture/status cases, 5 actual pipe-process cases and
3 pipe-2 unit cases passed after fixing the test child entry point. Tests cover
capture/enqueue crash-gap replay, stable sources, no duplicate commit, queued
status, worker-terminal status without a second lease, foreign/hash/policy denial,
closed packets, wrong kernel peer, native revoke/expiry and lost response.

Independent Python 3.12/jsonschema 4.26.0: 64 schemas pass, 4 new Owner wire
cases plus 2 actual preflight/write exchanges and a forged receipt refusal.
The actual status reply also passes the Rust subset schema validator.

Offline/locked serial workspace, --no-fail-fast: **852 passed, 0 failed, 4 ignored**.
That run precedes two equivalent if-let condition collapses and sharing the existing
test fixture module to satisfy Clippy; no business behavior changed afterwards.
Final-tree focused rerun: 6 new/old transport unit tests and 10 new/old actual pipe
tests passed. Fmt and strict workspace/all-target Clippy passed on the final tree.
Schema/validator files are unchanged since the successful independent cross-check,
so that evidence is reused. The workspace was not repeated after these equivalent
condition/test-import edits. Runtime surface was regenerated and checked; aggregate
`301adcb5968ac2bbfca70d3e6952b1b921a1dd10ce194416c938eda27b0d5e0a`, Workspace IPC remains 58 commands.
Local logs: target/mcp-segmented-validation/owner-wire-a-*.log (ignored).

## Failed attempts

The first test run had 51 Host unit passes and one failure: the new loss test
started pipe-1's test child, whose hello correctly failed against pipe-2. Corrected
only the test entry point; the rerun passed. Both Owner integration targets in
that first run passed. A fixture-generation command stopped on the local Python
default encoding, then was completed in UTF-8. The first independent transcript
check stopped on an unqualified Path name; corrected to pathlib.Path and reran
successfully. One tool approval timed out before execution and succeeded on its
single retry; it produced no implementation evidence.

Strict Clippy first refused two collapsible if-let blocks and duplicate loading
of the same shared test fixture. Conditions were collapsed without semantic
change; old/new unit tests now import one shared fixture. The final focused tests
and strict Clippy passed without lint suppression.

## Not delivered or proven

Background hosting is slice B. Real analyzer/model choice, budget and egress scope
need the owner's separate decision. No Runtime adoption/pin/hook, private data,
external Agent capture, memory_note, MCP-4/5, merge or deployment. Existing worker
limitations (single-source context, no entity/revision publication, deleted jobs
remaining forming, journal capacity and no old-binary rollback execution) remain.
No semantic quality or daily-use acceptance is claimed. Ambiguous analysis stages
automatically; no mandatory per-memory human queue is introduced.
