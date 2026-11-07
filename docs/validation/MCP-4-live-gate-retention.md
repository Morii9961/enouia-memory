# MCP-4 unknown live-gate packet retention

2026-11-04. Base `93a6deb`. Implements the existing ADR123–125 full-Value
retention contract; no new capability/schema or Owner authority.

If a native report writer became OutcomeUnknown before the next MCP handle call,
all three servers closed and returned no tool result, but original_unknown()
remained None. Three new focused tests reproduced that exact failure. The pipe
Client already retained its sent packet; the native service API was incomplete.

Each service now latches the first complete incoming Value in that live-gate
branch with get_or_insert_with. Later blocked calls keep it unchanged. Original
native request/key and current disconnect/revocation behavior are unchanged.
Tests cover StagingWritten and AfterCurrent publication faults, retained MCP ID,
method, arguments and _meta, no response, closed state and no replacement packet.

Actual checks, pinned Rust 1.98.1, offline/locked, serial Windows synthetic roots:

- Before the fix: all **3 new tests failed**, because original_unknown was None.
  Log: `target/mcp4-live-gate-repro.log`.
- After the fix: six affected Host native/server/pipe targets: **61 passed,
  0 failed** (14 correction, 16 supersede, 10 forget, three pipe targets of 7).
  Log: `target/mcp4-live-gate-tests.log`.
- Strict Clippy for Host lib and three changed native/server tests: passed.
  Log: `target/mcp4-live-gate-clippy.log`.
- Fmt, Runtime surface and whitespace checks: passed; surface
  `f7ea9ce2b4351deb743e4772474590fc1d736e768b62494b6ba25cb5385a94fb`.
- No schema/validator change, Python check or full workspace rerun. Older full
  counts remain evidence for their recorded source, not this patch.

Owned files: three MCP report service modules, three Host test files, this
report, continuation handoff and Runtime log/generated surface. No private
data, real Provider, model, Runtime repository, client configuration, AGENTS,
manual project status, deployment or merge. This synthetic regression is not
real-client adoption or Owner acceptance.
