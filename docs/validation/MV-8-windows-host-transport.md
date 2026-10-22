# MV-8.1h · Windows Host process and pipe prototype

Date: 2026-10-22 (Asia/Shanghai). Baseline
`0cb00a8a8e98eb1bbf5f6cefd8f0569c3314ce9e` (merged PR #45).
Continues owner-authorized MV-8 with isolated synthetic roots/processes only.

[ADR-MEM-77](../adr/077-windows-host-transport.md) adds the explicit prototype
executable, current-account/local-only reserved pipe, retained live-process
enrollment, reciprocal kernel PID verification, bounded/deadline I/O and
versioned native framing over the real HostService. The CLI launches no other
process and has no default/adoption/installation/Provider path. Test launchers
hide their own child windows and clean up only children they created.

The Host crate's 25 tests passed (including fixture child entry points):

- Existing four real Workspace-service and seven session/lifecycle tests.
- One encoded-byte budget/tag-role test, including escaping expansion.
- Four kernel pipe/process cases: first-instance exclusion and the pre-accept
  client race; timeout cancellation followed by successful next I/O; actual
  cross-process client identity and dead enrollment refusal; fixture entry.
- Nine process-transport cases: independent Host proposal/owner review/native
  lock/unlock/replay and competing Host refusal; oversized/truncated/claimed
  identity metadata; native control compatibility before ordinary session;
  raw overlay write/control refusal; a different live same-account process
  refused before handshake/Core; malformed reply channel poisoning with no
  subsequent bytes; and a three-process real disconnect while locked, fresh
  native unlock and original-key replay, including two fixture entry points.

The reconnect drill keeps one Host PID/lease across the physical pipe loss,
then exits the enrolled broker. The Host notices its retained process object's
exit and shuts down normally, after which a new coordinator can acquire the
root. Other failure cleanup may terminate only the test's own synthetic child;
it is not an OS/power-loss or unrelated-process cleanup claim.

Implementation review caught two transport obligations before final checks:
all uncertain/invalid client exchanges now poison the channel, and native
lock/unlock checks a closed versioned hello even when the ordinary gate is
locked. Synchronous I/O completion is handled without waiting for an event
that need not signal. The kernel cancellation test verifies the next read is
usable after draining an expired operation. No validator or owner scope was
weakened to pass a fixture.

Final checks:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 486 passed, zero failures; doctest groups passed with zero doctests |
| Strict workspace all-target Clippy | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface checker | Passed; recorded/logged `ff757421e8bccac01a6d283f88d898ab7ec6dc0d90f3b7a59959bdd993713643`, 55 commands |
| Public-content/diff review | 18 engineering/documentation/synthetic files; local links resolve, no matching private paths or credential shapes; concurrent user instructions/status snapshots excluded |

Logs and exit records are retained in ignored
`.local/mv8-{windows,transport}-*.log`/`.exit`.
Checks use pinned Rust 1.98.1 GNU, offline/locked Cargo and four test threads,
with incremental compilation/dev-test debug information disabled. Windows
bindings reuse pinned windows-sys 0.61.2; no external version was upgraded.
Schemas/validators are unchanged; prior independent 43-schema evidence is
reused. Framing compatibility metadata and the Runtime surface are regenerated.
Passing four-thread checks do not diagnose the historical default-parallel
ordered-barrier failures retained in earlier reports.

These are actual synthetic Windows process/IPC results. They do not establish
real Runtime/reference-shell transport adoption, executable/human attestation,
multi-client/MCP compatibility or production daily use. The explicit owner
launcher must choose the actual allowed process/window, not trust a JSON PID
or executable name; native brokers wrap page requests only as Call frames.
The prototype's caps may refuse otherwise-valid large Core payloads and its
full-data checks have no throughput acceptance. No private Vault, credentials,
real Provider call, Runtime checkout, deployment or manual status snapshot was
used or changed. Remaining gates include native reference-shell/client
adoption, enrollment UX/policy, multiple scoped clients, restricted MCP,
physical maintenance/independent encrypted restore and real owner acceptance.
