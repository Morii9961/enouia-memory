# Workspace history comparison v1

2026-10-23. Base: main `b0b891d7daa2aed49029b50b36cbef58335669e7`.
Scope: [ADR-MEM-81](../adr/081-workspace-history-comparison.md).
The existing provider helper and its comparison algorithm are unchanged.

The new page command resolves explicit task/batch selections natively and
preserves source coverage plus separate original and current candidate details.
No mutation, Provider configuration, request or automatic current-truth selection
is needed for comparison. A changed pin or disabled native owner state refuses
the read; errors retain safe codes/rule identifiers. Output is bounded and never
silently truncated.

Focused evidence:
- Contract Workspace tests: 6 passed. New request/response pairing and nine
  negatives cover empty/oversized/duplicate selection, caller identity, path,
  model/run injection and forbidden write key.
- Workspace Core comparison scenario: 1 passed, 85 filtered. Two admitted saved
  batches are produced only through fake ports, then the native Provider is
  removed. Reads preserve head and empty Canonical; each entry matches its
  saved batch's source text, quote, time, evidence and response hash. An explicit
  local application followed by a synthetic owner revision keeps the original
  statement/topic separate from edited candidate content/details and pending
  status. Reopen returns the same view. Unselected batch, revoked source-read
  policy and locked Vault are refused. Fake port counters stay at one secret
  read/exchange per saved batch.
- Independent installed python-jsonschema 4.26.0: 43 schemas, 145 Workspace
  messages; the full independent schema cross-check reports OK.
- Local logs: target/history-compare/workspace.txt and clippy.txt. An optional
  MEMORY_HISTORY_COMPARE_FIXTURE path exports only these synthetic result fields
  for separately run consumer checks; the test has no Runtime dependency.

Existing ADR-MEM-51 synthetic evidence remains evidence for unchanged topic,
branch, unknown-time, non-user-evidence, reviewed/withdrawn and limit semantics.
It is not represented as freshly rerun. The new native scenario does not prove
real model quality, owner acceptance, broad scale or physical UI behavior.
No full Rust workspace, frontend, package, smoke or performance rerun is required
for this local wire adapter. Host/MCP/cloud activation, personal sources,
credentials and manual project-status snapshots are outside this delivery.

Final quality checks: focused Workspace lib/tests Clippy with -D warnings,
format checks for both affected crates, four Runtime-surface tests and the
manifest/log checker pass. git diff --check passes. No dependency or lockfile
change; no private material is part of the public diff.
