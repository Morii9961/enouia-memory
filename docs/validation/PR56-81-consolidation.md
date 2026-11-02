# PR #56–#81 consolidation

2026-11-02. Morii authorized finishing and integrating the existing draft chain,
then cleaning merged branches and disposable development artifacts. The owner
confirmed that new work beyond #81 is outside this consolidation.

## What accumulated

The 26 draft PRs are dependency slices, not 26 unrelated abandoned tasks:

| PRs | Published scope |
| --- | --- |
| #56, #59, #61, #63, #68 | UI record layout, segmented catalog, projections, process/detail and Owner actions |
| #57, #58, #60, #62, #64–#66, #69–#75, #77 | Native Agent enrollment, versioned scoped reads, Runtime source capture and initialized-carrier adoption |
| #67, #76 | Synthetic initialization and the independent native empty Owner bootstrap |
| #78–#81 | Owner Runtime capture, durable queue, pipe-2 and cancelable background formation |

Each earlier slice stopped at its publication boundary without merging main.
The #81 head `b4f5213fd89652fd9cda64f2a59486fc9c550a48` already contains
25 PR heads. The remaining independent #76 head is
`fcda8049028d3ffab354b5010979f02493cd4552`. This integration preserves both
histories. Rust changes merge without conflicts; the ADR index and Runtime log
retain both additions, and the combined Runtime manifest is regenerated.

The 21 pre-existing uncommitted files were six modified documentation/rule
files and 15 new public research, design and handoff files, with no unfinished
Rust source among them. Their originals were preserved before checkout.
The public package is now tracked and marked historical, a stale ADR link is
corrected, and current engineering rules retain the later owner-data protection.
The previously authored project-status pair is retained at its original capture
time, without generating or refreshing a project status snapshot.

## Verification

Final combined source: **875 passed, 0 failed, 4 ignored**,
across 105 test/doc-test targets; the workspace command exited 0.
Fmt and strict workspace/all-target Clippy passed.
The checks use pinned Rust 1.98.1, offline/locked dependencies, one test thread,
new temporary synthetic roots and fake analyzers. No private root is opened.

Commands actually run: `cargo fmt --all -- --check`,
`cargo test --workspace --locked --no-fail-fast` with `RUST_TEST_THREADS=1`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`tools/schema-check/check_schemas.py` under the existing pinned Python environment,
and `python tools/integration/runtime_surface.py` after regeneration.
The initial shell lacked Cargo on PATH and ran no tests; the successful run used
the existing pinned Cargo executable. No dependency was installed.
The four existing ignored tests remain ignored; no soak, old-binary rollback,
real model, Runtime native UI or private-data acceptance was run.

Independent Python 3.12.10 / jsonschema 4.26.0 passed all 64 schemas and their
fixture groups. The generated Runtime surface matches its logged aggregate;
Workspace IPC remains 58 commands. Public historical-document links pass.

## Remaining work, outside this consolidation

- Runtime must independently adopt the exact Memory revision, Owner root/native
  adapters, read profiles, pipe-2, cancellation and foreground scheduling. A
  merged backend does not establish native UI or daily-use acceptance.
- Real model/analyzer/embedding, continuous policy, model choice, budgets and
  egress are not activated. Existing workers supply fake analyzers only. Real
  semantic quality, private initialization, independent recovery and production
  switching need their own scoped work and acceptance.
- Live formation still lacks multi-source context and daily entity/revision
  publication. Deleted sources can leave jobs forming; terminal-journal capacity,
  long-duration behavior and old-binary rollback remain unresolved or unproven.
  Unknown analysis stays terminal without automatic resend.
- Agent capture, memory_note and other MCP-4/5 writes/resources, plus cloud
  activation, are later slices. No new stage is started here.

See the [#81 report](MCP-3-captured-worker-host.md),
[bootstrap report](Owner-empty-bootstrap.md),
[Runtime integration duties](../integration/RUNTIME.md) and
[initialized-root adoption boundary](../integration/INITIALIZED_VAULT.md).

## Cleanup boundary

Only merged refs and explicitly identified build/test artifacts are disposable.
Tracked regression tests and synthetic fixtures remain in the repository.
Private intake, originals, owner-data markers, recovery bundles and uncertain
local material are preserved. A worktree containing private intake is retained;
it is not treated as disposable because its branch merged. Local preservation
and cleanup receipts stay ignored rather than entering public Git history.
