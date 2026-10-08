# Enouia Memory

Enouia Memory is the local-first, model-independent long-term memory system for Enouia. It holds recoverable raw history, reviewed canonical memory, sessions, and explainable context compilation. This repository is its **independent home**: design, machine contracts, domain code, tests, and versioned releases. [Enouia Runtime](https://github.com/Morii9961/enouia-runtime) is the Windows client and owns Activity. It hosts Memory's local frontend by embedding the workspace Core (`enouia-memory-workspace`) at a pinned Git revision behind its own adapter, and otherwise integrates through versioned contracts. Runtime does not own Memory code or data.

Status (2026-10-04): **MV-1 through MV-5 provide the local Memory foundation, verified with synthetic data in isolated roots. MV-6's Windows Memory Workspace baseline is implemented and merged ([report](docs/validation/MV-6.md)).** It includes memory browsing, import, candidate review, sessions, context inspection, recovery/status pages, tray, hotkey, and quick search. The [MV-6 follow-up](docs/validation/MV-6-followup.md) adds installation/startup/accessibility improvements, refuses silent downgrades, limits uninstall ownership, restricts quick search to native read permissions, and binds UI results/retries/drafts to their objects. **52/52 actual-app checks on synthetic data, 76/76 deferred-client UI checks, 201 Rust tests and 8/8 installer checks pass.** A test-only 0.1.1 package also passed upgrade/downgrade/Vault-preservation checks; the production version stays 0.1.0. Actual login startup, Narrator, a real Windows contrast theme, interactive installation/earlier-release upgrade and signing remain pending. Under [ADR-MEM-45](docs/adr/README.md), Runtime's Windows client is the product client for this local frontend, and `apps/workspace` is the reference shell where the results above were obtained; re-running W01–W05 on the Runtime host is Runtime's validation work ([integration](docs/integration/RUNTIME.md)).

Current MV-6 progress (2026-10-08): the Core hardening through `04792f1` was merged into `main` in [PR #5](https://github.com/Morii9961/enouia-memory/pull/5). It covers Vault lifecycle isolation, concurrent source/session/proposal replay, bounded import reads and complete purge confirmation results. See the [current status and next steps](docs/validation/MV-6-current-status.md) and the individual [reports](docs/README.md). Runtime must adopt the combined revision and reset its corresponding UI state as recorded in the [compatibility log](docs/integration/RUNTIME.md). This is synthetic Core evidence; the October 4 actual-app counts above do not validate this revision.

A real export drill (I07), encrypted backup with restic, OS-crash and power-loss evidence, and the gaps in the stage reports remain pending. **MV-7 text Provider functionality is implemented (2026-10-08), with real smoke deferred by the owner.** OpenAI Responses and Anthropic Messages have native adapters, explicit egress inspection/approval, bounded calls, durable results, quotas and optional source-bound candidate extraction ([report](docs/validation/MV-7.md), [native integration](docs/integration/PROVIDERS.md)). The workspace UI still uses offline Mock; real-account connectivity, Runtime adoption, embedding, MCP and VPS remain pending. All committed fixtures are synthetic. See the [implementation plan](docs/design/IMPLEMENTATION_PLAN.md) and [stage reports](docs/README.md).

## Layout

```text
docs/        all documentation: design/, adr/, contracts/, integration/, validation/, reviews/, handoff/, history/
contracts/   JSON Schema 2020-12 machine contracts (memory, context, provider, ipc, store)
crates/      Rust workspace: contract, vault, import, govern, index, context, provider, workspace Core, and CLI
apps/workspace/  reference Tauri shell and React frontend, and the acceptance harness (not the product client)
tests/       synthetic fixtures
```

Start with [docs/README.md](docs/README.md).

## Runtime integration

Enouia Runtime's Windows client is the product client for Memory's local part: it embeds `enouia-memory-workspace` at a pinned Git revision and reaches it only through workspace IPC v1 and its own adapter ([ADR-MEM-45](docs/adr/README.md)). `apps/workspace` stays here as the reference shell and acceptance harness; new product UI for the local part lands in Runtime first. A change to any file Runtime depends on must regenerate `docs/integration/runtime-surface.json` and add a row to the compatibility log in [docs/integration/RUNTIME.md](docs/integration/RUNTIME.md), saying what Runtime must adopt, in the same commit; `cargo test` fails until it does. The cloud stages (the MV-7 Provider, the MV-8 Host and MCP, the MV-9 gateway and queue, MV-10 replicas, MV-11) and their contracts stay in this repository.

## Building

Rust 1.98.1 (`stable-x86_64-pc-windows-gnu`), offline (`CARGO_NET_OFFLINE=true`); the checks are listed in [AGENTS.md](AGENTS.md). Since MV-4 the index compiles SQLite from its bundled source, so a MinGW C compiler is needed as well (for example MSYS2 UCRT64 `gcc`). Put the compiler's `bin` directory before the Rust toolchain's in `PATH`: otherwise `cc1` loads the toolchain's older `libgcc_s_seh-1.dll` and the SQLite build fails. The built binaries need no extra DLL.

## Data boundaries

Real Memory data never lives in this repository. The CLI requires an explicit data root; the reference shell takes one through its native folder picker or `--vault`. It does not automatically create a default root or remember the last location, and Runtime's client is bound by the same [host duties](docs/integration/RUNTIME.md#host-duties). Raw exports, attachments, sessions, databases, keys, backups, and recovery material are excluded by `.gitignore` and must never be committed.

## License

Not yet chosen. Until the owner selects a license, all rights are reserved.
