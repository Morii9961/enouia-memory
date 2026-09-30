# Enouia Memory

Enouia Memory is the local-first, model-independent long-term memory system for Enouia. It holds recoverable raw history, reviewed canonical memory, sessions, and explainable context compilation. This repository is its **independent home**: design, machine contracts, domain code, tests, and versioned releases. [Enouia Runtime](https://github.com/Morii9961/enouia-runtime) (the Windows client and Activity) integrates with it through versioned contracts. Runtime does not own Memory code or data.

Status: **MV-1 (Vault and recovery foundation, [report](docs/validation/MV-1.md)), MV-2 (history import and rescue, [report](docs/validation/MV-2.md)), MV-3 (segmented catalog, candidates, owner review, deletion, [report](docs/validation/MV-3.md)), and MV-4 (SQLite/FTS5 index and literal search, [report](docs/validation/MV-4.md)) are complete for synthetic data in isolated roots.** A real export drill (I07), encrypted backup with restic, OS-crash and power-loss evidence, and the gaps named in the MV-3 and MV-4 reports are pending. MV-5 has not started and needs the owner's explicit go-ahead. No UI, model call, embedding, MCP, or VPS exists, and no real data root has been created. All fixtures are synthetic.

## Layout

```text
docs/        all documentation: design/, adr/, contracts/, validation/, reviews/, handoff/, history/
contracts/   JSON Schema 2020-12 machine contracts (memory, context, provider, ipc)
crates/      Rust workspace: enouia-memory-contract (pure contracts), enouia-memory-vault (store), enouia-memory-import (history import), enouia-memory-govern (candidates and review), enouia-memory-index (SQLite/FTS5 search), enouia-memory-cli (local entry point)
tests/       synthetic fixtures
```

Start with [docs/README.md](docs/README.md).

## Building

Rust 1.98.1 (`stable-x86_64-pc-windows-gnu`), offline (`CARGO_NET_OFFLINE=true`); the checks are listed in [AGENTS.md](AGENTS.md). Since MV-4 the index compiles SQLite from its bundled source, so a MinGW C compiler is needed as well (for example MSYS2 UCRT64 `gcc`). Put the compiler's `bin` directory before the Rust toolchain's in `PATH`: otherwise `cc1` loads the toolchain's older `libgcc_s_seh-1.dll` and the SQLite build fails. The built binaries need no extra DLL.

## Data boundaries

Real Memory data never lives in this repository. The default runtime data root is `%LOCALAPPDATA%\EnouiaMemory`; an explicit, verified local path can replace it. Raw exports, attachments, sessions, databases, keys, backups, and recovery material are excluded by `.gitignore` and must never be committed.

## License

Not yet chosen. Until the owner selects a license, all rights are reserved.
