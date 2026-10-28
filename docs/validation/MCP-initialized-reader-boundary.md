# MCP initialized-root adoption checkpoint 1

2026-10-28. Branch codex/mcp-owner-root-boundary; MCP base
210fbc31062713aa0392466909e988fc13ae8f27. Merge exact published dependency
34709b9d2abcb60188c0e3488b15f40c33d6ee16, preserving its ADR100/101 and Owner
history. ADR-MEM-105 / MCP_INITIALIZED_AGENT_READ_V1. This is a refusal/integration
checkpoint, not a delivered initialized-root Agent reader.

## Changes and support matrix

The Core can create/open the new empty Owner layout with explicit subject and
genesis descriptor binding. Existing Agent catalog factory now checks its exact
old layout before snapshot inspection. Existing Runtime capture also checks its
exact old layout before policy/source planning; otherwise integrating the new
Core layout would silently widen its previous contract. Native Owner gesture/
consent methods remain outside MCP, as do System read ports.

| Root / consumer | Result |
| --- | --- |
| schema 1 / host-current-v2 | Existing original six-tool semantics |
| schema 2 / automation-host-v2/v3 | Existing separately negotiated read capabilities |
| schema 2 / segmented-host-v1, original carrier | Existing five tools and source-only Runtime capture |
| automation-segmented-owner-v1 via old Agent factory/delegation | Denied at descriptor before snapshot/query/count |
| automation-segmented-owner-v1 via Runtime capture v1 | Denied before transaction planning |
| Owner root via non-segmented AutomationVault | Denied before lease/record decoding |
| Initialization import/attachment extension | Not adopted here |

Resolve additive dependency, schema-checker, plan/register and Runtime-log merge
conflicts by retaining both sides. Regenerate the surface rather than select a
stale generated file. Keep the previously extracted no-write commit_checked
helper; Owner delta mutation belongs in that shared implementation, not the
thin commit wrapper. The first compile exposed this mutable-binding mismatch;
it was fixed without changing public System methods or native publication rules.

## Actual verification

Two new native integration tests passed. Real legal Owner genesis/current policy
is refused by the old factories without changing any business bytes. A separate
owned root has a deliberately corrupted witness to prove denial precedes current
snapshot/policy/record inspection; no successful receipt is fabricated. Existing
RuntimeEvent roots still independently delegate Agent reads and Client capture,
and mismatched Agent/System roles refuse. These are native factory tests, not
new-root MCP transport acceptance.

The focused four-target run passed 27 tests, zero failed, one existing ignored
scale test: new root boundary, published Owner transactions, actual separate-
process Runtime pipe and existing Agent five-tool pipe regression. Pinned
Python 3.12 / python-jsonschema 4.26.0 independently passed 60 schemas and the
combined corpora, including six actual fresh Agent pipe exchanges and one
invalid actual output. A fresh Runtime pipe exchange test also passed; the
independent checker accepted its two actual preflight/capture replies and
rejected one forged formation response. Strict offline/locked all-target
workspace clippy passed.

The first full attempt stopped at the integration-log test: the manifest had
already been generated when testing started, but its matching RUNTIME.md entry
was added afterward. This was verification ordering, not a relaxed contract.
All four runtime_surface tests then passed with the entry present. A separate
one-shot full rerun uses owner-boundary-workspace-final.*; the earlier failed
owner-boundary-workspace.* evidence is retained.

The separate final complete offline/locked run exited 0 with a full doctest
tail: 775 passed, zero failed, four existing ignored scale tests. This includes
the native Owner-consent compile-fail doctest; ignored scale tests are not
reported as rerun. Evidence is owner-boundary-workspace-final.txt/.exit under
the ignored validation directory. The one-shot helper exited. Pinned Rust
1.98.1 fmt, Runtime manifest/log checks and the exact 99-file public allowlist
scan passed; no outside file or credential-shaped addition was found.

Surface generation/log records aggregate
31e2b482d57a4d085d55bc4b226674c7a00bc90a0a57b3c677afa99e61c8392e.
Evidence is retained under ignored target/mcp-segmented-validation/owner-boundary-*
and the explicit owned-file allowlist. Only disposable synthetic roots and owned
test processes are used. No other worktree, AGENTS or manual status is edited.

## Requirements 03 differences and next work

The published initialization dependency is now
2c7310a81cd086b17c702d916d1d8ea51a75eb6a; it is not merged by this checkpoint.
New native catalog-4/process-v2 support does not advertise an Agent reader or
upgrade an old pin. Next adapt a separate initialized-root Agent view with
current policy/source/deletion/withdrawal qualification, safe Owner overlays and
visible ordinals, exact tool output schemas and the same Context/Agent trace.
Then bind Runtime-only capture and optional Agent hints to the published shared
ordered live formation entry. No mandatory per-memory owner queue is created.

No new MCP write tool, Runtime hook/pin/UI, Provider/embedding, private Vault,
real client configuration, remote listener, deployment or main merge is claimed.
