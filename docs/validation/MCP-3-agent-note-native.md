# MCP-3 native Agent note preparation

2026-11-03. Base `17e6df2`. [ADR-MEM-115](../adr/115-native-agent-note-drafts.md)
and [native contract](../contracts/AGENT_NOTE_NATIVE_V1.md).

This preparatory slice grants a separate opaque native note permit, binds an
optional Agent interpretation to one exact currently authorized Runtime source,
uses the pinned quote locator and then rechecks strict original evidence. The
draft records actual source role/class, authenticated Agent provenance, current
delegation/snapshot and deterministic principal/request identity. The Host holds
its lifecycle gate during preparation. Existing read permits cannot be supplied
as note permits and pipe-5 still advertises only five reads.

Owned files: Vault segmented `agent.rs` and exports; Host `qualified_agent.rs`
and `tests/agent_note.rs`; ADR115 and register; native note contract; this report;
Runtime integration log and generated surface. No dependency or schema change.

The focused command `cargo test -p enouia-memory-host --test agent_note --locked
--no-fail-fast` passed **3 tests, 0 failures**, using new synthetic roots under
ordinary Windows permissions. Coverage includes:

- Normalized full-width/quoted/whitespace hints map to exact original scalar
  offsets. Both user statements and assistant model claims retain their original
  role/class; Agent notes never become Owner confirmation.
- Repeated preparation retains its key/hash; changed content changes only its
  request hash, and another authenticated Agent has a different key. Preparation
  changes no chain head, creates no hint journal or queue entry, and forms no
  memory. These are drafts, so no durable idempotency result is claimed.
- Explicit native Owner setup, initialized profile and source-read permission
  are required. Revoke, lock/unlock generation replacement, policy revision,
  expiry, deletion and another instance refuse. The uninitialized original
  read-only Host profile cannot create note delegations.
- Repeated quote matches, absent quotes, oversized/empty fields, unknown source,
  wrong revision, unselected source and a forged UserConfirmation evidence class
  all refuse before publication.

`cargo check -p enouia-memory-host --locked` and the focused tests passed on the
final implementation. Pinned Rust 1.98.1, offline/locked dependencies and serial
tests were used. No dependency was installed, no private source was opened and no
model or actual MCP client was connected.

Final combined workspace checks: **882 passed, 0 failed, 4 ignored**, across 106
test/doc-test targets; `cargo test --workspace --locked --no-fail-fast` exited 0.
`cargo fmt --all -- --check` and strict workspace/all-target Clippy also passed
on that implementation. This run includes both deleted-source worker fixes.
The Runtime surface and whitespace checks passed. Aggregate:
`59128df59ed3151e75af017e156871daeb583d8b768a15c9b92ab666ba21c1c4`.
Ignored local logs: target/continuation-workspace-tests.log and
target/continuation-workspace-clippy.log. No schema cross-check was run because
this native-only slice changes no schema or validator.

This is not the advertised `memory_note` tool. There is no persisted note,
saved-hint receipt, hint consumer, new transport, mandatory draft/review queue or
formation side effect. Durable intake and its authority/idempotency/recovery
contract must be frozen before implementing submission. Runtime adoption and
real model quality remain separate.
