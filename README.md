# Enouia Memory

Enouia Memory is the local-first, model-independent long-term memory system for Enouia. It holds recoverable raw history, reviewed canonical memory, sessions, and explainable context compilation. This repository is its **independent home**: design, machine contracts, domain code, tests, and versioned releases. [Enouia Runtime](https://github.com/Morii9961/enouia-runtime) (the Windows client and Activity) integrates with it through versioned contracts. Runtime does not own Memory code or data.

Status: **MV-1 (Vault and recovery foundation) is complete for synthetic data in isolated roots ([report](docs/validation/MV-1.md)); encrypted backup with restic, OS-crash, and power-loss evidence are pending. MV-2 has not started and needs the owner's explicit go-ahead.** No importer, index, UI, model call, MCP, or VPS exists, and no real data root has been created. All fixtures are synthetic.

## Layout

```text
docs/        all documentation: design/, adr/, contracts/, validation/, reviews/, handoff/, history/
contracts/   JSON Schema 2020-12 machine contracts (memory, context, provider, ipc)
crates/      Rust workspace: enouia-memory-contract (pure contracts), enouia-memory-vault (store), enouia-memory-cli (local entry point)
tests/       synthetic fixtures
```

Start with [docs/README.md](docs/README.md).

## Data boundaries

Real Memory data never lives in this repository. The default runtime data root is `%LOCALAPPDATA%\EnouiaMemory`; an explicit, verified local path can replace it. Raw exports, attachments, sessions, databases, keys, backups, and recovery material are excluded by `.gitignore` and must never be committed.

## License

Not yet chosen. Until the owner selects a license, all rights are reserved.
