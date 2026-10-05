# Runtime integration (ADR-MEM-45)

Enouia Runtime ([Morii9961/enouia-runtime](https://github.com/Morii9961/enouia-runtime)) is the Windows client. It hosts Memory's local frontend by embedding `enouia-memory-workspace` at a pinned Git revision behind its own adapter. This repository keeps the domain, the contracts, the data rules, the reference shell, the CLI, and every cloud stage. This page is the handoff. It covers what Runtime consumes, what each side owns, how a change here reaches Runtime, and the compatibility log.

The Runtime-side counterpart is Runtime [ADR-025](https://github.com/Morii9961/enouia-runtime/blob/main/docs/adr/025-enouia-memory-integration.md) with its [integration guide](https://github.com/Morii9961/enouia-runtime/blob/main/docs/MEMORY_INTEGRATION_v1.md). Runtime records the pinned revision in `docs/integration/memory-pin.json`.

## Ownership

| Item | Owner | Notes |
|---|---|---|
| Contracts, Vault format, import, review rules, index, Context, Sessions, Mock (`crates/*`, `contracts/*`) | Memory | Runtime never calls these crates directly. |
| Workspace Core (`enouia-memory-workspace`) and [workspace IPC v1](../../contracts/ipc/workspace-v1.schema.json) | Memory | The one seam Runtime consumes. |
| `HostSurface` caller scope (`enouia-memory-contract::workspace`) | Memory | Every host applies it before forwarding. |
| Product Windows client: Memory, Context and Sessions surfaces, native windows, picker adapter, lifecycle (`apps/desktop`) | Runtime | Built on the pinned Core. |
| Reference shell and acceptance harness (`apps/workspace`, `apps/workspace/e2e`, `tools/windows`) | Memory | Not the product client. New product UI lands in Runtime first. |
| CLI (`enouia-memory-cli`) | Memory | The independent recovery path: the only real `restore`. |
| MV-7 Provider, MV-8 Host and MCP, MV-9 gateway and queue, MV-10 replicas, MV-11 | Memory | Runtime implements none of them. It hosts UI and clients only. |
| Activity & Usage | Runtime | Never enters workspace IPC. Memory never reads it. |

## What Runtime consumes

- **API.** `Workspace::new(Config::default())`, `call(&Value) -> Value`, `register_pick(PickKind, &Path)`, `open_root(&Path)`, `set_companion(Value)`, `shutdown()`, and `HostSurface`. Other `pub` items in the crate (`Fail`, `ops`, `views`, `purge_key`, `PICK_TTL_MS`) carry no stability promise.
- **Wire.** The workspace IPC v1 envelope: request `{schemaVersion: 1, requestId, command, idempotencyKey, arguments}`; response `{schemaVersion, requestId, kind, vaultCommitId, operationId, result, error}`. There are 36 commands; the ten write commands carry an idempotency key and every other command sends `null`. The schema constrains requests and the command-to-kind pairing only. `result` fields are built by the Core, and some embed stored snake_case records. Runtime keeps its own tests for the fields it renders.
- **Build.** Rust 1.98.1 `x86_64-pc-windows-gnu`, edition 2024. Exact pins `serde =1.0.229`, `serde_json =1.0.151`, `windows-sys =0.61.2`, `rusqlite =0.40.2` (bundled SQLite: a MinGW C compiler whose `bin` comes before the Rust toolchain's in `PATH`), `crc32fast =1.5.2`, `miniz_oxide =0.8.9`. Changing a pin here changes Runtime's resolution.
- **Fixtures.** `tests/fixtures/memory/workspace-manifest.json` has one valid request per command plus responses and negative cases. All of it is synthetic.

## Host duties

Every host (the reference shell, Runtime's adapter, a test harness) must:

1. **Keep paths native.** Only native code calls `register_pick` (after a native dialog) or `open_root` (a trusted command-line argument). The page gets `{token, displayName, bytes}` and never a path. Pass plain, canonical, absolute paths. A Vault root or backup target must not sit under a Git working tree, a sync folder, Program Files, or Windows.
2. **Scope by native identity.** Map each native window to a `HostSurface` and forward only when `surface.allows(&request)`. Never derive the surface from request fields. Give the quick-search surface its own capability, without picker, exit, or startup commands.
3. **Keep work off the UI thread.** `call` blocks; run it on a worker (`spawn_blocking`). Report host failures (scope denial, worker panic) outside the Memory error codes.
4. **Serialize lifecycle.** Run `vault_open`, `vault_create`, `vault_lock`, and `shutdown` exclusively of other calls, and call `shutdown()` on every exit path. The Core has no `Drop`, and `close` only joins the operations that exist when it starts.
5. **Retry honestly.** Reuse an idempotency key only to resend the same payload. Retry only when `retryable` is true. `revision_conflict`, a stale cursor, or an unknown plan means re-read or re-plan.
6. **Render safely.** Use a strict CSP (`default-src 'self'`, `connect-src ipc: http://ipc.localhost`, no remote resources) and no `fs`, `shell`, `http`, or dialog plugin. Render memory and source text as plain text nodes only (`source_excerpt.untrusted` is always true). Never log request or response bodies, and persist no copy of memory text.
7. **Open roots explicitly.** Do not default, remember, or auto-open a Vault root. Creating a Vault needs the typed phrase `create new vault`, and the Core enforces it.
8. **Keep Activity out.** Replace the Core's fixed Activity status row (`component: "activity"`, `state: "unavailable"`, `mode: "independent_not_managed"`) with the host's own Activity health. Never send Activity data through `call`.
9. **Run one Core per Vault.** Do not run the reference shell and Runtime on the same Vault at once (see ADR-MEM-45).

## Routing a change to Runtime

`docs/integration/runtime-surface.json` lists every file Runtime depends on:

- the wire contract
- the Core crate's sources
- the build closure of the Core
- the reference shell
- the reference frontend

`crates/enouia-memory-contract/tests/runtime_surface.rs` fails when any listed file changes, or when a new file appears in a covered directory. It also fails while the new aggregate is missing from the log below. When it fails:

1. Keep the change transport-neutral: no window toolkit, Runtime, or Activity dependency in a domain crate (the boundary tests enforce this).
2. Run `python tools/integration/runtime_surface.py --write`. It prints the new aggregate.
3. In the same commit, add a row to the log below with that aggregate. Say what changed and whether it is additive or breaking for Runtime, and name exactly what Runtime must adopt.
4. Run the AGENTS.md checks, then commit and push.
5. On the Runtime side, bump the revision with Runtime's runbook (`docs/MEMORY_INTEGRATION_v1.md`): the `rev` in `apps/desktop/src-tauri/Cargo.toml`, the lockfile, `docs/integration/memory-pin.json`, then the adapter and surfaces, then re-validation. Until then Runtime keeps running the previous revision.

**Breaking versus additive.**
- **Breaking:** a removed or renamed command, field, or error code, a changed meaning, a new required argument, a changed `Workspace` signature, or a new exact pin.
- **Additive:** a new optional result field or a new command.
- A breaking wire change needs a new `schemaVersion`, or a Runtime change prepared for the same revision. Record which in the log.

**Changes outside the surface.** A behavior change in a deeper domain crate (vault, import, govern, index, context) reaches Runtime only when it bumps the pin. Before each bump, Runtime reviews `git log <old>..<new> -- crates contracts apps/workspace`.

**Cloud changes.** Cloud-stage changes are not part of this surface: `contracts/ipc/memory-v1.schema.json`, provider capabilities, Host, MCP, gateway, and replica code. If MV-8 replaces the embedded Core with the Memory Host, that is a breaking surface change. It is logged here together with the Runtime migration from embedding to a Host client.

## Known Core behavior hosts should handle

These were found while preparing the handoff. They are Memory-side follow-ups and are not changed by ADR-MEM-45.

- `review_confirm` removes its plan even when the commit fails. A retryable `busy` answer therefore leads to `not_found` / `workspace.plan_unknown` on retry, and the owner must plan again.
- `vault_create`, `import_start`, and `backup_export` consume their token before the work starts. `import_start` and `import_resume` ignore the idempotency key. After a lost response, a retry gets `workspace.token_unknown`; recover the running import through `operation_list`.
- Every `verify_data_root` rejection collapses to `invalid_request` / `workspace.root_rejected`. A host can only list the common causes: inside a Git working tree, a sync folder, not NTFS/ReFS, or not a fixed disk.
- The index is taken with `try_lock`. Two concurrent index users (search, ask, preview) get a retryable `index_not_ready` / `index.busy`.
- `vault_verify` and `backup_export` cannot be cancelled, so a lock or exit waits for them.
- A panicking operation stays `running` and can poison the index until the Vault is locked and unlocked.
- There is no version handshake. The pinned revision and `schemaVersion: 1` are the only version facts. `workspace_status` reports no build version.

## Compatibility log

Each row records the surface aggregate printed by `tools/integration/runtime_surface.py`. The newest row is last.

| Date | Surface aggregate | Change | Runtime follow-up |
|---|---|---|---|
| 2026-10-04 | `a499b083913a4dc84b24eaccdb98fe4dd197cece6cac5589d2e1f1b9959aba3a` | Initial surface (ADR-MEM-45): workspace IPC v1 (36 commands), the Core API above, the build closure, the reference shell and frontend. Adds `HostSurface`, which the reference shell now uses for its window scope. | Runtime ADR-025: embed the Core at this revision in `apps/desktop`, map Runtime's main window to `HostSurface::Workspace`, port the reference client behavior, and record the pin in `docs/integration/memory-pin.json`. |
