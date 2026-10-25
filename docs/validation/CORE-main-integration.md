# Core refactor main integration

2026-10-25. Morii explicitly authorized merging the completed core refactor.

Integrates Core-1 through Core-7 (`674b069`) with main `aa10f1a`, preserving
the current Provider hardening, offline seed bridge, Host/MCP delivery and all
58 Workspace v1 commands. Only integration log/manifest conflicts required
resolution; implementation merges without source conflict. No global schema
version or default writer change. Core stages remain bounded by their original
validation reports; synthetic checks do not establish real model quality or
personal migration readiness.

Owned files are the original Core change set plus this integration report and
regenerated integration records. Borrowed untracked design/research/handoff
packages, AGENTS, manual status snapshots, private material and parallel trees
are excluded. No private Vault, account, credential, Provider or embedding is
opened. Old v1 roots and original owner approval history remain intact.

## Validation

Integrated pinned Rust 1.98.1 / offline / locked checks passed:
`cargo fmt --all -- --check`, `cargo test --workspace --locked` (661 tests,
zero failures, `RUST_TEST_THREADS=4`) and `cargo clippy --workspace --all-targets
--locked -- -D warnings`. Independent Python 3.12.10 / jsonschema 4.26.0 passed
all 50 schemas and the existing record/set/IPC plus Core-1..7 fixture groups.
The normal system Python lacked jsonschema; the existing pinned validation
virtual environment was used without changing product code or dependencies.

Regenerated/checked Runtime surface:
`01cfc998a6707897b1956ef31248dbd44102578440ff558095ab5566917f5b82`,
all 58 Workspace v1 commands retained and logged in RUNTIME.md. Public content,
owned-file bounds, conflict markers and whitespace were checked before push.

## Initialization and activation

Merging source does not initialize or change any existing library. Existing v1
roots remain supported by their v1 entry points. Using the isolated automation
format requires a separately selected new-format root and a migration scope;
never reset the original root or reinterpret old owner records as System
approvals. Production capture/authentication, real-model quality, fixed source
snapshot, independent recovery, source/history/deletion consistency and old
primary fencing remain separate activation gates. Core-8 currently creates
fresh synthetic examples only; that is neither personal initialization nor a
production cutover. No merge authorizes deployment or data destruction.
