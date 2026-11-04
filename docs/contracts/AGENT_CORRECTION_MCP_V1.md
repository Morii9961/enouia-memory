# Agent correction-report MCP v1

2026-11-04. [ADR123](../adr/123-agent-correction-mcp-transport.md).

Explicit server: five initialized reads plus memory_correction_report. Native
setup supplies independently live initialized reader and correction reporter
for the same Agent and exact leased Vault. No packet selects a principal,
permit, scope, PID or Owner. Existing notes/read profiles do not gain this tool.

Closed input: schema_version=2, kind=agent_correction_request,
capability=memory-agent-correction-1, request=the original
AgentCorrectionRequest. Target, source, quote_hint, correction and request_id
keep [native](AGENT_CORRECTION_NATIVE_V1.md) and
[intake](AGENT_CORRECTION_INTAKE_V1.md) bounds and authority. The untrusted Agent
interpretation is never a source, user evidence or content.

Closed output: schema_version=2, kind=agent_correction_response,
capability=memory-agent-correction-1, request_id, receipt and error. Success:
the original AgentCorrectionReceipt with state=report_saved and error=null.
Known refusal: receipt=null and error={code,retryable:false}; closed codes are
invalid_request, permission_denied, conflict, stale_scope, capacity, busy,
recovery_required and unavailable. No interpretation/source body, principal,
native scope hash, result memory or global sequence is echoed. TargetRef inside
the existing receipt remains the reported target, not a corrected-memory result.

Read tool schemas/authority stay unchanged. One session owns initialization,
MCP request-ID uniqueness and bounded request count. The correction tool is a
native write; tool metadata cannot grant it. Advertised schemas have local
references only. Original domain keys remain stable across explicit replay.

OutcomeUnknown returns no tool response: close, retain the entire original MCP
Value and disconnect before Reply. Block further calls and revoke the paired
delegations. Explicit recovery plus fresh enrollment may replay the original
domain request/key. No automatic retry/reconnect/new key or mandatory human
review queue. Successful storage and synthetic tests do not imply real System
model quality, Runtime adoption or Owner acceptance.
