# Workspace local history navigation brief v1

2026-10-23. Stacked on comparison source 20cbb2f. Existing ADR-MEM-52 text
generation and its controlled-source evidence are unchanged.

Focused evidence:
- Six Workspace contract tests pass, including the new request/kind pairing and
  eight negative cases for selection, budgets, arbitrary actor/topic/path and key.
- The extended single comparison/brief Core scenario passes (85 filtered).
  Fake ports are removed before local reads. Brief base/native selection matches
  the inspected comparison, exact UTF-8 hash matches, original suggestion and
  current edited candidate wording both remain. Uncited coverage is retained.
  A one-byte budget refuses instead of truncating; the old pre-edit head refuses.
  Reads leave head unchanged and lock refuses the brief.
- Independent installed python-jsonschema: 43 schemas, 155 Workspace messages,
  all independent cross-checks OK.
- Logs: target/history-brief/workspace.txt, contract.txt and schema.txt. Optional
  MEMORY_HISTORY_BRIEF_FIXTURE saves only synthetic fields for a separate
  consumer check; Memory has no Runtime dependency.

This is local deterministic navigation evidence. It does not prove model quality,
personal-history coverage, physical UI, owner acceptance, exported-file handling
or production readiness. No full Rust workspace/frontend/package/smoke/performance
rerun, real sources/credentials/Provider activity, manual status update or new
Host/MCP/cloud stage is included.

Affected Workspace lib/tests strict Clippy and both affected-crate format checks pass. The new integration aggregate is regenerated and recorded in the same slice; four focused surface tests and manifest/log checks pass.
