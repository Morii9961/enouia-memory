# MV-8.2e · Agent provisional session checkpoints

Date: 2026-10-23 (Asia/Shanghai). Implementation baseline `9aa66b0`, branch
`codex/mv8-mcp-boundary`. This completes only the current checkpoint item within
Morii's explicit MV-8.2 local synthetic authorization. The latest instruction is
to commit/push this item and stop. [ADR-MEM-84](../adr/084-host-agent-checkpoints.md)
defines the boundary.

## Delivered behavior

The existing Host accepts SessionPropose without borrowing an owner window or
Workspace identity. Current persisted policy authorizes the session, every
selected event and each source's current/cited revision. A checkpoint uses only
events on the requested branch within the explicit inclusive sequence range;
both endpoints must exist. Parent branch history and outside-coverage item
references cannot enter silently. Stored coverage contains exact event IDs and
their hash. IDs/timestamps are generated natively; generated_by identifies the
Agent, and the artifact stays provisional without a review or approval.

The native Agent entry binds the original logical range, summary and items to a
separate durable key namespace. Generated item IDs are excluded from the key
payload. Explicit replay returns the original checkpoint revision, commit and
operation after unrelated appends; changed body/range/items conflict. The
existing owner API retains its payload, receipt namespace and whole-branch
coverage. Original session events and Canonical memory are unchanged.

Admission/final auditing is content-free. Actual publication faults close the
MCP session and return the original packet/key without resend. After a complete
Host witness, a fresh explicit native admission recovers the original receipt
without a second checkpoint. Incomplete publication remains fenced, without MCP
repair. Replayed stale artifacts produce a known revision_conflict; deleted or
non-provisional artifacts cannot be returned as currently provisional.

Two unfinished-code edges were corrected before verification: replay no longer
sets the new-publication flag unconditionally, and successful replay followed
by final-audit failure remains outcome_unknown. Current artifact visibility is
also checked against tombstones before delivery.

## Checks actually run

| Check | Result |
| --- | --- |
| Six new focused actual Host checkpoint tests | Passed: default deny/exact coverage/Agent provenance/owner compatibility; stable receipt after append and changed-payload conflicts; missing endpoints, foreign session and inherited/outside references refused; independent source permission and policy revocation on replay; stale receipt known conflict; actual AfterCurrent and HostWitnessPublished fault boundaries |
| `cargo test -p enouia-memory-host -p enouia-memory-context --locked -- --test-threads=4` | Host **61** + Context **15** passed, zero failures; zero doctests. Reuses existing read/propose/Context, native lifecycle and owned pipe/process checks |
| `cargo clippy -p enouia-memory-host -p enouia-memory-context --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface regeneration/check and `cargo test -p enouia-memory-contract --test runtime_surface --locked` | Passed, **4** guards; same **55** Workspace commands; current aggregate logged in RUNTIME.md |

All fixtures use independently created synthetic temporary roots and owned
child processes. No Computer Use, window/tray interaction, private Vault,
credential access, real Provider, Runtime or Activity check was performed.
No full-workspace test/build/Clippy or schema check was required.

## Exit table

| Exit | Evidence / status |
| --- | --- |
| Native in-process six-tool binding | Delivered: search/read/source, pending propose, limited current-memory Context and this checkpoint binding; earlier reports retain their historical scope |
| Current-policy checkpoint admission and exact provisional artifact | Delivered and synthetically tested; no owner identity or event fabrication |
| Durable checkpoint receipt and unknown-outcome handling | Delivered and synthetically tested, including actual publication fault points |
| Broader Context selections | Not satisfied: historical/session/branch/egress selections remain explicitly refused; not started by this item |
| External native Agent authentication/enrollment and runnable transport | Not satisfied: Grant is a native trust boundary, not OS authentication; the protocol-only synthetic stdio example does not open a Vault |
| Real MCP clients, installation and personal acceptance | Not satisfied; synthetic in-process evidence does not establish these exits |
| Reference-shell actual window/tray acceptance and Runtime Host adoption | Remain separate/pending; existing Runtime embedded product integration is already present |

## Stable Runtime statement and delivery boundary

The historical-results minimum remains
`9d68d3d8707749ad3b0e2a36cfc97d71a27e712f` / **55 Workspace commands** /
`c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`.
This additive native Host/session entry does not require a Runtime pin, command,
storage, version, transport or product UI change. Current development surface
aggregate is `afbbaa936a787f199cc5d94942e253d2c6d80e9d7b0ce980f8afba9e1626daa4`.

Publish only the feature branch after public-content review; no merge,
deployment, next item or next MV stage. Owner-modified AGENTS/status files and
old experimental drafts remain untouched and unstaged. Reference-shell success
and local synthetic MCP success do not imply Runtime product adoption or
personal acceptance.
