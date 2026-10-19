# MV-7 · Read-only history planning host entry

Date: 2026-10-19. Starting revision: `56bc5eb8b08fb80cd6df892bf53d200ff23afc4c`.
Scope: [ADR-MEM-66](../adr/066-read-only-history-planning-host.md), optional
`Workspace::plan_history` and its [Runtime contract](../integration/HISTORY.md).

The Core now consumes completed archived imports directly, with an observed
Vault/commit/import revision and explicit unit/branch/size selection. It verifies
archive hash/size, adapter/member/unit agreement and current source metadata,
checks source-read inspection permission, logical/physical deletion and final
freshness, then returns bounded text-free coverage and batch references. The
shared lifecycle gate excludes lock/switch/shutdown. No durable record, job,
Provider invocation or new page command is created.

## Evidence

Synthetic native-host checks cover branch order/preceding context, dated and
unknown times, parse and attachment gaps, explicit selection, stable IDs,
read-only commits, absence of source text/upstream IDs, invalid/oversized
selection, wrong Vault/import/revision, overlay/no Vault/locked/moved head,
source revision mismatch/rebinding, source-read permission, malformed graphs,
concurrent revision invalidation, memory forgetting versus source purge,
serialized-result budget and lifecycle exclusion.

The first sandboxed run failed during existing temporary Vault ACL setup,
before the new method ran. The same checks passed with native ACL access.
An initial deletion test wrongly expected logical memory forgetting to remove
its source; the existing delete implementation deliberately preserves it. The
test now checks that distinction, then checks source/raw purge before and after
physical file removal. No deletion product behavior was changed.

Final checks used Rust 1.98.1 GNU, offline Cargo and Python 3.12.10 with
python-jsonschema 4.26.0, from this repository root:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked` | Passed: 405 tests, zero failures, including all 10 new host checks; default test concurrency |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `python tools/schema-check/check_schemas.py` | Passed: 36 schemas, 55 valid records, 119 record cases, 4,913 set records, 51 IPC messages, 83 workspace messages, 46 store documents, 8 Provider invocation and 19 extraction-job cases |
| `python tools/integration/runtime_surface.py` | Passed; recorded/logged aggregate `8d2d9e2d5c4ada8064bcc73c985f5c35c4c9c7d5d8741e7849e687119b1fef5a`; 47 commands |
| Diff and public-content review | Passed; no added private paths, credential-shaped strings or personal absolute paths |

Full Rust and Clippy output is retained in ignored `.local/history-host-workspace.log`
and `.local/history-host-clippy.log`. A passing default-concurrency run does not
explain or fix the previously recorded ordered-barrier scheduling failures.

## Remaining work

Runtime must pin this Core, implement a native caller-scoped adapter and present
explicit planning selection and result invalidation. No Runtime checkout was
changed. The 47-command IPC, storage schemas and dependencies are unchanged.
Batch materialization, semantic/extraction/review product integration, real
Provider/personal-history quality, full-scale archived reads, independent backup
recovery and owner acceptance are not established by these synthetic checks.
MV-8/cloud/MCP and manual project status snapshots remain unchanged.
