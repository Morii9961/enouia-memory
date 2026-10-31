# MCP-3 Owner-root Runtime capture native

2026-10-30. Base 87fd505. Only newly allocated synthetic roots and owned child
processes. No private root or bootstrap candidate is consumed.

## Actual focused evidence

20 focused cases passed across five new Owner-native cases, two original-native
cases, five original Runtime pipe cases and eight initialized MCP pipe cases.
The native Owner factory retains a real owned Runtime process peer and grants
Client only. Old root/new profile, Owner root/old profile, Agent producer, foreign
subject policy and mismatched request capability all refuse. Old pipe-1 refuses
the new native delegation before connection binding/business decoding.

Owner preflight preserves business files byte-for-byte and the snapshot. A legal
capture publishes source revisions; replay returns original sources without a
new snapshot, changed packet under the same key conflicts, and process exit,
revocation or response loss closes access. Unknown retains the exact request;
no automatic resend/reconnect or replacement key. Old config-hash/replay behavior
is unchanged. Actual native source publication receipts validate against the
separate Owner-native request/receipt schema; they report formation_scheduled=false.

The same initialized Agent reader reads the captured sources. Missing original
time/timezone stays missing; stored default timezone is Asia/Shanghai with
owner_default origin. Typed missing attachment metadata and source refs match;
no bytes/hash/detected type or verified size is claimed. Upstream size/hash
claims remain raw audit data, not verified metadata. Source text/roles/model/
language/conversation bindings retain the existing native capture rules.

An initial fixture omitted its required nullable attachment content_hash and
correctly refused; the fixture was corrected without relaxing validation. A
receipt check initially used a request-only schema; the new Owner contract now
includes an explicit receipt branch, preserving all old schemas and protocols.
Failed evidence is retained and not counted as a pass.

Pinned Python 3.12/jsonschema 4.26.0 passed 63 schemas and twelve Owner capture
request cases. Fmt, strict workspace/all-target clippy and the offline/locked
serial workspace run passed: 828 passed, 0 failed, 4 ignored. Surface aggregate:
b2e97f6cb5a6d13f98d0bbff95b66e2593e860290ef5c82a8bb6ca70654ca9b0;
Workspace IPC v1 remains 58 commands.

## Support matrix and remaining work

| Root | Native producer | Local transport / Agent write |
| --- | --- | --- |
| segmented-host-v1 | Existing Runtime Client native capture | Existing pipe-1 only |
| automation-segmented-owner-v1 | Explicit memory-runtime-owner-capture-native-1, bound Owner subject | New Runtime wire pending; pipe-1 refuses |
| Owner sources with typed missing attachments | Initialized native/pipe-5 Agent reader | Read-only; no Agent capture/Owner action |
| Other layout, forged profile/producer or invalid current scope | Refuse | No authority |

Native Core authorization is not OS authentication; RuntimeCaptureHost.enroll_owner
separately retains the actual peer. No borrowed Workspace/System reader or second
writer. Source capture does not produce jobs/memories, and no manual uncertainty
queue is introduced. Next work is an explicit Owner Runtime transport and queue-only
admission of exact captured SourceRevisionRefs/original keys to the same System
formation rules, with live priority and per-conversation FIFO. No import recapture.
Runtime hook/config/pin/UI, real Provider/embedding, private data, new Agent note/
correction/forget tools and actual daily-formation acceptance remain undelivered.
