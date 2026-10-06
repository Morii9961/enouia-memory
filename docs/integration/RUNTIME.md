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
3. **Keep work off the UI thread.** `call` and `shutdown` block; run them on workers (`spawn_blocking`), including native menu lock/exit callbacks. Exit the host after cleanup finishes. The reference shell uses `src-tauri/src/lifecycle.rs`; page exit awaits its shutdown worker and tray callbacks queue workers before returning. Report host failures (scope denial, worker panic) outside the Memory error codes.
4. **Serialize lifecycle.** Run `vault_open`, `vault_create`, `vault_lock`, `vault_unlock` and `shutdown` exclusively of other calls, and call `shutdown()` on every exit path. The Core now also excludes lifecycle changes from ordinary page calls (including their response commit binding) and native token registration. Keep the host gate for UI/action sequencing. `workspace_status` and `operation_get`/`operation_list`/`operation_cancel` bypass the Core gate, so progress and cancellation remain available while close waits for calls and joins workers. The Core has no `Drop`. After close, discard the old Vault's operation IDs, plans, retries and import/backup/export tokens in the host UI; old operation IDs answer `workspace.operation_unknown`. Root picker tokens remain valid until success or their existing expiry. `workspace.lifecycle_failed` is not retryable: stop new actions and recreate the Core after cleanup.
5. **Retry honestly.** Reuse an idempotency key only to resend the same payload. Retry only when `retryable` is true. `revision_conflict`, a stale cursor, or an unknown plan means re-read or re-plan.
6. **Render safely.** Use a strict CSP (`default-src 'self'`, `connect-src ipc: http://ipc.localhost`, no remote resources) and no `fs`, `shell`, `http`, or dialog plugin. Render memory and source text as plain text nodes only (`source_excerpt.untrusted` is always true). Never log request or response bodies, and persist no copy of memory text.
7. **Open roots explicitly.** Do not default, remember, or auto-open a Vault root. Creating a Vault needs the typed phrase `create new vault`, and the Core enforces it.
8. **Keep Activity out.** Replace the Core's fixed Activity status row (`component: "activity"`, `state: "unavailable"`, `mode: "independent_not_managed"`) with the host's own Activity health. Never send Activity data through `call`.
9. **One Core per Vault.** The Core enforces this (ADR-MEM-46): a second embedded Core gets `busy` / `workspace.vault_in_use`, which is not retryable. Tell the owner the Vault is open in another app.

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

[ADR-MEM-46](../adr/README.md) fixed these items, which were found during the handoff:

- dropped confirm plans
- consumed tokens on failure
- unkeyed import retries
- hidden root reasons
- operations stuck as `running` after a panic
- poisoned indexes
- unguarded shared Vaults
- closed-Vault confirmation replies, operation results, backup status and data picker tokens remaining reachable ([lifecycle evidence](../validation/MV-6-vault-lifecycle.md))

What remains:

- **Keyless `vault_create` and `backup_export`.** Their requests carry no idempotency key, so a retry after a lost response that had succeeded gets `workspace.token_unknown`. Recover through `workspace_status` (`lastBackup`, the open Vault) or `operation_list` while the same Vault remains open. `lastBackup` and operation history are in-memory state since that open, and reset on lock, root switch or shutdown; they do not prove that an earlier backup never happened.
- **Index contention.** The index is taken with `try_lock`. Two concurrent index users (search, ask, preview) get a retryable `index_not_ready` / `index.busy`.
- **Picker contention.** While a command validates or schedules a selected token, another use of that token gets retryable `busy` / `workspace.token_busy`. Wait for the original response before retrying: success may consume the token, and expiry continues from selection. Import preview is reusable; a failed command before scheduling preserves its token. Worker failure after successful scheduling does not restore it.
- **Uncancellable operations.** `vault_verify` and `backup_export` cannot be cancelled, so a lock or exit waits for them. Keep status and progress reads outside any host-side lifecycle gate.
- **No version handshake.** The pinned revision and `schemaVersion: 1` are the only version facts. `workspace_status` reports no build version.
- **Remember replay.** A published remember receipt binds both text and `claimKey`. Reusing its request key with different input reports `workspace.key_reuse`; assign a new key when editing input. Replayed proposal results describe the original action, so refresh candidates/memories for current review status. The broader governance proposal/edit/withdraw replay paths still need request-binding review; this guard covers Core `remember` only.
- **Concurrent source replay.** The manual-assertion service now returns the published receipt's source ID when `commit` replays another concurrent same-key call. The [finding and correction](../validation/MV-6-pending-source-replay.md) covers a deterministic synthetic helper regression. The full concurrent remember path and other services still need their own checks; hosts should serialize simultaneous submissions using the same key.
- **Concurrent Context session replay.** Core's Context services restore published IDs when start/append/fork/checkpoint commits replay a concurrent call. Session/branch IDs come from the receipt's original revision. This does not make full concurrent `session_ask` or changed transport request IDs interchangeable; keep host sequencing and validate those separately.
- **Source byte budgets.** `source_excerpt` returns non-retryable `invalid_request` / `workspace.excerpt_budget` if the next UTF-8 character cannot fit the requested budget. Increase that budget rather than retrying identical input. A budget of at least four bytes fits a single character; the reference page already uses 4096. Successful pages before EOF advance `byteEnd`; paginate using `byteEnd < totalBytes`, as `truncated` also describes an omitted prefix.

## Compatibility log

Each row records the surface aggregate printed by `tools/integration/runtime_surface.py`. The newest row is last.

| Date | Surface aggregate | Change | Runtime follow-up |
|---|---|---|---|
| 2026-10-04 | `a499b083913a4dc84b24eaccdb98fe4dd197cece6cac5589d2e1f1b9959aba3a` | Initial surface (ADR-MEM-45): workspace IPC v1 (36 commands), the Core API above, the build closure, the reference shell and frontend. Adds `HostSurface`, which the reference shell now uses for its window scope. | Runtime ADR-025: embed the Core at this revision in `apps/desktop`, map Runtime's main window to `HostSurface::Workspace`, port the reference client behavior, and record the pin in `docs/integration/memory-pin.json`. |
| 2026-10-05 | `4e5d44297e52e63c65ee3a79926152a8b8aa6b99db93299462fe546ea8daa8c7` | ADR-MEM-46, additive for pages: root rejection reasons (`root.*`); picker tokens kept after failed commands; `review_confirm` keeps its plan on retryable failures; keyed `import_start`/`import_resume` replay (`workspace.key_reuse` on conflict); one embedded Core per Vault (`busy` / `workspace.vault_in_use`, not retryable; the lock follows open/close, a refused unlock stays locked, unlocking an open Vault answers its status); panicking operations end `failed` (`operation.panicked`); poisoned index reopened. The reference shell names the new rules. Envelope, commands and `Workspace` API unchanged. | Bump the pin. Show `workspace.vault_in_use` and the `root.*` reasons as owner-readable text, without a Retry for `vault_in_use`. The adapter's lifecycle gate stays valid; a Retry on a busy confirm now succeeds. |
| 2026-10-05 | `65d7ae8c115ad4d38c1702243df7f2ba4ff4f4593356dd0b486a74aa71dcbf36` | ADR-MEM-46 lifecycle follow-up: after workers join, close forgets confirmed replies, operation history, last backup and import/backup/export tokens; root tokens retain success-only consumption. Confirm replay requires an open Vault and the original diff hash. Wire envelope and public Core API unchanged; behavior breaks hosts relying on closed-Vault state. | Update the host behavior with the pin bump: discard old Vault UI state on lock/root switch/close; treat `workspace.operation_unknown` and `workspace.plan_unknown` as expired state, and re-pick expired data tokens. Label `lastBackup` as since this Vault was opened. Keep progress/status polling available during join. Re-run host lifecycle acceptance; this repository's Core tests do not establish Runtime adoption. |
| 2026-10-05 | `34ce4f550c7d23909f8794b8a1128bff15a64e127841498d3a3043d12f79e8f8` | ADR-MEM-46 additive concurrency guard: lifecycle changes, native opens and shutdown exclude in-flight ordinary calls and token registration. Read calls remain concurrent; status/progress/cancel bypass the guard. Response commit binding stays in the guarded call. A poisoned lifecycle writer refuses new work with non-retryable `storage_failed` / `workspace.lifecycle_failed`; shutdown still attempts cleanup. Wire/API unchanged. | Bump the pin; preserve host lifecycle/UI sequencing, including unlock. Keep status and operation controls outside the host's exclusive gate. On `workspace.lifecycle_failed`, stop actions and recreate the Core after cleanup. Validate concurrent call/lock/exit behavior in the Runtime host; local Core tests are synthetic. |
| 2026-10-05 | `8f6dbe664c7cb4333b5f775a150c78b806e5da95d6a2280502b7b4521ea4a9cd` | ADR-MEM-46 corrective task outcomes: a cancel request no longer turns unrelated worker failures into `cancelled`; a worker cancellation error reports `cancelled`; late cancel requests leave terminal tasks unchanged. Completion and cancellation admission share the outcome lock. Wire/API unchanged; behavioral correction for hosts equating a request with a result. | Bump the pin. Use terminal `state` and its error/result to explain the outcome; `cancelRequested` expresses an accepted request only. A task may succeed or fail after a request. Do not offer Cancel or assume it changes a terminal outcome. Revalidate host task/error messaging. |
| 2026-10-05 | `4a39ccdddc84a7b9a024edc085ce2b0c0f5e424524caa3e9edc143397a0d5f5a` | Reference-host lifecycle scheduling: tray lock/exit queue blocking workers; page exit awaits a shutdown worker; the process-exit callback follows cleanup. Five synthetic scheduling checks include detached tray handles. Wire and Core API unchanged. | Keep lock/exit cleanup off Runtime's native event thread and exit only after it completes. Adopt the reference scheduling pattern where applicable and validate actual menu/page responsiveness in Runtime. The synthetic helper tests do not establish Runtime or real-window behavior. |
| 2026-10-05 | `27a5cf198c9cf5dcd641e87894cb979265af972365eb36a16cd1bbd383b851db` | Corrective picker admission: atomically reserve each token across validation/scheduling; concurrent reuse returns existing retryable `busy` with new rule `workspace.token_busy`. Failure/unwind releases admission, consuming success removes the token, preview remains reusable, expiry is unchanged. Four synthetic tests cover the shared guard and actual page routes. Wire/API unchanged. | Bump the pin; serialize UI submissions using one choice and handle `workspace.token_busy` as a pending use, without automatically scheduling a duplicate. Await the first response, then recover state or re-pick consumed/expired tokens. Preserve keyed import retries. Validate host contention/retry behavior; this is local synthetic evidence. |
| 2026-10-05 | `c8c4a1ff482c8449469664aad537adc1a4888ce68a79aed793a690d9851cf7a2` | Corrective source pagination: insufficient UTF-8 budget returns existing non-retryable `invalid_request` with new rule `workspace.excerpt_budget`. Successful pages before EOF advance and respect the byte cap. EOF and start rounding remain unchanged; wire/API unchanged. | Bump the pin. Use a source budget of at least four bytes within the existing cap, or explain the new rule and let the caller increase it. Paginate by `byteEnd < totalBytes`; validate mixed-language excerpts in Runtime. The reference UI's existing 4096-byte budget needs no change. |
| 2026-10-05 | `e493a75c9018391b353ae3f03a674297c08aba9a6bc35c5c10e5e1b8a33116cf` | Corrective Core remember replay: published receipts bind text and `claimKey` using their original candidate revision; pre/post admission checks reject mismatched reuse with existing `idempotency_conflict` / `workspace.key_reuse`. Identical retries remain valid after review/reopen. Wire/API/storage format unchanged. | Bump the pin; keep keys for identical retries and assign new keys when the text or claim changes. Refresh current candidate/review status after replay. This narrow guard does not establish complete domain-wide payload binding or atomic source/proposal creation. |
| 2026-10-05 | `22676a68dfe16985678219347921a82ca711670cf6b27dd47aa964e0617fee49` | Manual assertion service correction: a commit-time replay returns the published source ID. A permanent deterministic Core helper regression forces concurrent initial lookups. Wire/API/storage format unchanged; deeper Vault service behavior changes with the pin. | Bump the pin. Continue to serialize same-key submissions: full concurrent remember, other service returns and broader governance payload binding remain outside this correction's proof. Validate the Runtime adapter's replay behavior. |
| 2026-10-06 | `f20c2eec11d022e3c7a6ccdf9efdaa012be674e51a35964f7b366ec2ef9fa292` | Context session replay correction: commit-time replays return the published session/branch/event/checkpoint IDs; historical receipt revisions preserve original branches after later forks. Four deterministic regressions include actual concurrent `session_new` page calls. Wire/API/storage format unchanged; Context behavior changes with the pin. | Bump the pin and refresh sessions using returned published IDs. Preserve host sequencing. Separately validate full `session_ask`, fresh request IDs on transport retry and different-key contention. These remain outside this slice's proof. |
