# MV-8.1e · Native Host domain-writer binding

Date: 2026-10-22 (Asia/Shanghai). Baseline
`54b017c011ce869045ca3af68d2d0493fdeb2242` (merged PR #42).
Continues owner-authorized MV-8 using synthetic local roots only.

[ADR-MEM-74](../adr/074-native-host-domain-writer.md) gives existing `&Vault`
domain services an explicit native writer. Binding verifies an active v2
primary and owns the OS lease until drop, capturing identity plus epoch.
Commits use fresh authority/data verification and witnessed publication;
pins and receipt-only replay paths also check. Legacy open/create and fenced
maintenance/export retain their prior behavior.

Five focused Windows tests passed: explicit adoption/native identity/lease,
synthetic lifecycle commits/checkpoints, source-service reopen replay followed
by witness-corruption refusal, interrupted publication and exact recovery,
and old-epoch refusal after coherent A→B→A transitions. That last fixture
deliberately replaces synthetic control files despite a live lease to exercise
freshness; it is not distributed fencing or whole-filesystem rollback proof.

One governance integration test passed with the real domain APIs: Agent
proposals remain pending, Agent review is refused, owner plan/confirm creates
Canonical memory, and exactly two authority publications accompany propose/
confirm. The unbound legacy handle still cannot publish. Initial compilation
used incomplete existing view arguments; supplying the actual pin and
Canonical selector fixed the test without changing governance implementation.

Final checks:

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-vault --locked` | 88 passed, zero failures; includes the five new Host-writer tests |
| Focused governance Host integration | 1 passed, 8 unrelated review tests filtered out |
| Strict workspace all-target Clippy | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface checker | Recorded/logged `1f990f2857c3cd94949d993252a72b3bfd290d728c3898a7080f5dbdd85ad6b3`; 55 commands |
| Public-content/diff review | 11 engineering/documentation/synthetic files; local links resolve, no matching private paths or credential shapes; instructions/status excluded |

Logs/exit codes are retained in ignored
`.local/mv8-host-writer-{focused,vault,govern,clippy}.*`. Pinned Rust
1.98.1 GNU, offline/locked Cargo, four test threads and disabled incremental
compilation/dev-test debug information match the recovery baseline.
Schemas/validators are unchanged, so its independent Schema evidence is
reused. The preceding full 454-test run is baseline evidence, not a claim of
another full workspace run for this slice.

Next remain the user-level Host process, authenticated native sessions,
version handshake, revocation and reference-shell reconnect, then restricted
MCP. Host dispatch must gate the entire operation/lifecycle; an old pin is no
authorization grant. Stop/drop the bound writer for control/recovery
maintenance. Host-aware physical deletion/export, independent encrypted
restore, Runtime adoption, real Provider acceptance, migration and cloud
operation remain separate. No private data, credentials, Runtime checkout or
manual project snapshot was used or changed.
