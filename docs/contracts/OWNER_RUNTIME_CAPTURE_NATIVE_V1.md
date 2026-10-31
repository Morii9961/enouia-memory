# Native Owner-root Runtime capture v1

2026-10-30. ADR102 continuation.

Explicit native capability memory-runtime-owner-capture-native-1, only on
Owner layout automation-segmented-owner-v1. The old native capture and pipe-1
remain old-layout-only. Trusted native Owner setup registers a retained OS
Runtime peer (Client, never Agent/System), exact active policy/source-pool,
subject matching descriptor.owner_subject_id, sensitivity, account and timezone.
The constructor alone is not OS enrollment; Host enroll_owner performs it.

Request and permit profiles must match. Preserve original raw turns, source
IDs/revisions/locators, request key/hash, missing original time/timezone and
owner-default provenance. No auto resend/reconnect or replacement key on unknown.
New typed attachment records preserve source/ref/name/claimed media metadata;
bytes/hash/detected type are absent and availability is missing. Unverified size
claims remain audit data, never a verified size/hash. Original policy/config
hashes and old-layout replay semantics remain unchanged.

Preflight is no-write. Capture returns genuine source publication only, with
formation_scheduled=false. No shared queue/formation success is fabricated. Later
queue-only admission must consume exact captured SourceRevisionRefs and original
keys, not recapture as import or borrow Owner confirmation. No new MCP Agent
writer, Owner/System port, Runtime repository/configuration or private root use.
