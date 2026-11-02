# Runtime capture native contract v1

MCP-3 checkpoint 1, ADR-MEM-102. Native implementation/evidence are recorded in
the corresponding validation report; this document grants no producer authority.

Only schema 2 / automation-segmented-host-v1. Keep the frozen UI record fields.
No in-place upgrade or private default root. Missing initialization is a known
native setup condition, never a fabricated captured/queued receipt.

Native Owner setup selects a Client Runtime principal, exact active policy ref,
subject/project, at most 128 source IDs already in that policy, sensitivity,
opaque account alias, default timezone and at most five minutes. The Host must
independently enroll/verify the retained Runtime OS peer. Ordinary Agent grants,
clientInfo, caller source IDs, policy fields or principal/PID parameters cannot
register a producer. Source-ID capacity exhaustion refuses; it does not broaden
the policy or create a per-memory Owner review queue.

Closed capture packets contain schema_version 2, runtime_capture_request kind,
memory-runtime-capture-native-1 capability, request_id, original conversation ID,
optional title and up to eight turns. Each turn has original message/parent and
branch IDs, positive sequence, speaker, exact UTF-8 text (at most 16 KiB), optional
original time/timezone and normalized occurred_at, language, optional assistant
model and bounded attachment metadata. No identity/scope/knowledge/confirmation
fields are accepted. Metadata-only attachments do not claim captured bytes.
Their descriptors remain in the typed source extension; attachment_refs stays
empty and attachment_bytes_captured is false. No managed blob is invented.

Preserve the raw text and original metadata. Backend-generated RuntimeEvent
locators, source/branch/session/turn identities and current SourceRecordFields
bind them to the same witnessed source transaction and conversation catalog.
Only observed User gets UserStatement; all machine roles get ModelClaim, never
UserConfirmation. A message identity cannot silently acquire changed content.

`original_timezone` remains exactly the supplied value or null. The typed source
extension `runtime.capture` records the native producer, request/message hashes,
resolved_timezone and timezone_origin (`source` or `owner_default`). Missing
timezone resolves to the registered Asia/Shanghai default with its native
configuration hash. This is provenance, never an authorization or Owner action.
Missing occurred_at remains null/time_precision unknown; capture time is not
substituted as event time. Date basis in MemoryRecordFields remains independent.

Preflight exposes a prospective native plan and writes no business data. Source
capture returns evidence from the real System capture transaction; replay must
verify current policy, source deletion and exact original payload. Partial or
changed work conflicts. The first checkpoint schedules no formation and exposes
no public MCP capture tool. Durable live jobs, same-conversation ordering,
normalized quote lookup for memory_note and real local Runtime transport are
the next bindings, before claiming the complete daily automatic path.

Native result observed_in is the current verified snapshot, not a promise of
the original commit on later replay. Producer request keys are checked across
the root before reuse, including a narrower re-enrollment. An existing message
under a different request key conflicts; cross-key coalescing is not supplied.
Interrupted publication requires explicit native recovery and reconciliation;
neither the producer nor ordinary open performs that recovery automatically.
