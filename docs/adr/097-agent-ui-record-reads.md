# ADR-MEM-97 — Explicit Agent reads of frozen UI records

2026-10-26. MCP-2b, authorized by Morii's continuous MCP development request.
Depends on UI record contract commit 24a03e02bdf18094d60bc4d3268d380bd1b669a1.

Add native `agent_record_reader` for automation-host-v3, independently of the
HostV2 reader. Keep schema 2 but require memory-agent-record-read-1 and the exact
layout/capability/schema-hash handshake. Existing v1/v2 clients refuse this root
before business queries; no source-pin or major-only fallback grants support.

Preserve the sealed current-policy/source/subject/project/sensitivity delegation,
OS process enrollment and lifecycle barriers. Entity names and every alias must
also fit the same policy, source revision/deletion and Agent source scope before
an item can contribute to any result/count. Reuse only authorized pure projection,
never a System native port. No Owner action, writer or broader Context is added.

Return stored claim/time/formation fields plus authorized current EntityRecord
metadata, keyed by the same memory revision as each page/detail/recall item.
Return SourceRecordFields only with independent source-read permission. Unknown
time stays unknown; title/language, quoted speaker/scalar ranges, precision/bound/
timezone/basis, formation provenance and annotations are never fabricated.
Charge these fields to output and recall budgets. Native Context keeps the same
actual computed recall response; reads do not reinforce attention or persist it.

This is bounded HostV3 support. The parallel segmented layout remains unsupported
until its committed native interfaces are explicitly adopted. Synthetic temporary
roots are sufficient: no private initialization, provider/embedding, real-client
configuration, Runtime edit, production switch or deployment is needed or claimed.
Runtime-only capture and Asia/Shanghai default provenance remain MCP-3 inputs.


Suggested relation hints are not authoritative claim dependencies. Project them
to current authorized target revisions before serialization/rendering and always
declare annotations_scope=authorized_current_targets. Return no hidden-target
counts/flags and do not modify stored annotations or suppress an otherwise
readable claim because a hint names a hidden or stale target.
