# Existing MCP/Host branch: current-main integration

2026-10-24. Morii authorized eligible existing-work merges, without app/Host/MCP
activation, real clients, personal data, credentials, Provider calls or UI redesign.
Base main: 32ef2235b5d2016e48e3cf6b41847c42ef2fb3a3.
Existing implementation: 9286cd8f89669d9ec3b92f943fd74072457536e7.

The existing six-tool in-process Agent binding stays bounded by native grants,
persisted current record/source policies, deletion/provenance checks, distinct
receipt namespaces and original-packet retention after uncertain publication.
Owner Workspace permissions are not borrowed. No external authentication,
enrollment, real-client transport or new stage is activated by this merge.

Two generated/log merge conflicts were reconciled while retaining both histories.
Semantic ADR collisions were resolved without changing main's history ADR81-83:
Host reads/proposals/local Context now use ADR87-89; stdio remains80 and
checkpoint84. Existing parallel Core work's85/86 is not edited or merged.
See ../handoff/MCP_MAINLINE_ADR_MAP_2026-10-24.md.

Workspace remains58 commands. The combined manifest aggregate is:
f4db930d6d32fb65486ae81964d3557c5762613969b84f8c6425a29757dfddf1.
Host declares only already locked internal crates/serde; no external package
version or wire/storage schema changes. Runtime keeps08892702 /57 commands.

Checks actually run on this combined source:
- Host library27 and MCP protocol8 tests passed. These use in-process synthetic
  services/fault points; no runnable Host/MCP application or real client started.
- Context continuity15 tests passed.
- Five reference-shell Host API tests and TypeScript passed, reusing an existing
  matching Vite dependency installation through an ignored junction. No install.
  The seven-line reference App action-error change preserves the immutable
  original request; it is not a Runtime or Claude redesign change.
- Strict Host/Context/Govern lib/tests Clippy, affected-crate format checks,
  four Runtime-surface tests, manifest/log and targeted ADR links passed.
- Local ignored logs: target/mcp-boundary-integration/host.txt, context.txt,
  host-api.txt, typescript.txt, clippy.txt and surface.txt.
- Unchanged Provider integration evidence from#52 is reused. No full workspace,
  full frontend, build/package, smoke, performance or independent Python schema
  rerun is claimed. No new tests were added.

Real Agent authentication/enrollment, broader Context choices, actual Runtime
Host adoption, physical shell/tray exit, personal quality and owner acceptance
remain open. Concurrent Core/MCP original worktrees, all Runtime redesign
branches/worktrees and manual status snapshots are preserved.

Reference Host closeout follow-up: the final documentation-only commit
d6abbb4a45c0b0072cba8f42107497a2363b1148 is also included. Its first four
ancestors were already shared by the MCP branch; only the final report cutoff
is new. No implementation/surface change follows from that merge, and no
behavior check is rerun for it. Its original no-merge statements describe its
2026-10-23 delivery, superseded only for merging by Morii's current request.
