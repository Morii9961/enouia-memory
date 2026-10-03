# Enouia Memory

Enouia Memory is the local-first, model-independent long-term memory system for Enouia. It holds recoverable raw history, reviewed canonical memory, sessions, and explainable context compilation. This repository is its **independent home**: design, machine contracts, domain code, tests, and versioned releases. [Enouia Runtime](https://github.com/Morii9961/enouia-runtime) (the Windows client and Activity) integrates with it through versioned contracts. Runtime does not own Memory code or data.

Status (2026-10-03): **MV-1 through MV-5 provide the local Memory foundation, verified with synthetic data in isolated roots. MV-6's Windows Memory Workspace baseline is implemented and merged ([report](docs/validation/MV-6.md)).** It includes memory browsing, import, candidate review, sessions, context inspection, recovery/status pages, tray, hotkey, and quick search. This branch's [MV-6 follow-up](docs/validation/MV-6-followup.md) adds a Windows installer, opt-in startup, safe write retries and keyboard/accessibility improvements: **43/43 synthetic real-app checks, 196 Rust tests and 8/8 installer checks pass.** Actual login startup, Narrator, a real Windows contrast theme, interactive installation/upgrade and signing remain pending.

A real export drill (I07), encrypted backup with restic, OS-crash and power-loss evidence, and the gaps in the stage reports remain pending. **MV-7 has not started.** Responses still use the offline Mock; real model calls, embedding, MCP, and VPS integration are not implemented. All committed fixtures are synthetic. See the [implementation plan](docs/design/IMPLEMENTATION_PLAN.md) and [stage reports](docs/README.md).

## Layout

```text
docs/        all documentation: design/, adr/, contracts/, validation/, reviews/, handoff/, history/
contracts/   JSON Schema 2020-12 machine contracts (memory, context, provider, ipc)
crates/      Rust workspace: contract, vault, import, govern, index, context, workspace Core, and CLI
apps/workspace/  React frontend and Tauri Windows shell
tests/       synthetic fixtures
```

Start with [docs/README.md](docs/README.md).

## Building

Rust 1.98.1 (`stable-x86_64-pc-windows-gnu`), offline (`CARGO_NET_OFFLINE=true`); the checks are listed in [AGENTS.md](AGENTS.md). Since MV-4 the index compiles SQLite from its bundled source, so a MinGW C compiler is needed as well (for example MSYS2 UCRT64 `gcc`). Put the compiler's `bin` directory before the Rust toolchain's in `PATH`: otherwise `cc1` loads the toolchain's older `libgcc_s_seh-1.dll` and the SQLite build fails. The built binaries need no extra DLL.

## Data boundaries

Real Memory data never lives in this repository. The CLI requires an explicit data root; the Windows app takes one through its native folder picker or `--vault`. It does not automatically create a default root or remember the last location. Raw exports, attachments, sessions, databases, keys, backups, and recovery material are excluded by `.gitignore` and must never be committed.

## License

Not yet chosen. Until the owner selects a license, all rights are reserved.
