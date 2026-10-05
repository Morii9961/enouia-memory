# MV-6 Core — lifecycle concurrency

Date: 2026-10-05. Base: `0a6b6e5`. End: the feature commit containing this report. Scope: MV-6 / ADR-MEM-46; no Runtime checkout changes and no MV-7 work.

## Problem and reproduction

Host duties required lifecycle serialization, but the reference shell forwards page calls on independent blocking workers and its tray uses the same Core directly. Core close could detach and release a Vault while an ordinary call still retained an `Open` handle. The earlier lifecycle cleanup did not exclude these calls. A channel-controlled Clock paused an actual `memory_list` after it acquired the Vault; on the base commit, `vault_lock` completed before the read was released. The failing assertion was `lock passed an in-flight Vault read`.

## Change

A transport-neutral read/write gate now covers ordinary request routing and response commit binding. Native picker registration shares the read side. `vault_open`, `vault_create`, `vault_lock`, `vault_unlock`, native `open_root` and `shutdown` take the exclusive side. Internal open/close helpers do not re-enter the gate. Concurrent ordinary calls still share the gate; background operations remain cancellable workers, joined by close.

`workspace_status`, `operation_get`, `operation_list` and `operation_cancel` bypass the gate. They remain available while a lifecycle call waits for an ordinary call or joins workers. Hosts must retain their own UI/action sequencing and stale-response protections; Core exclusion does not prevent an already constructed response from arriving late at a renderer.

An isolated poisoned lifecycle writer refuses ordinary calls, native opens and token registration with non-retryable `storage_failed` / `workspace.lifecycle_failed`. Status/task observation stays available; shutdown attempts cleanup even when the gate is poisoned. Recreate the Core after cleanup. This is fail-closed gate handling, not general recovery from arbitrary panics or poisoned domain mutexes.

## Evidence

- `lifecycle_waits_for_an_in_flight_page_call`: a paused real read and a paused real `remember` write, each against lock, root switch, failed native open and shutdown. Lifecycle completion waits; status stays readable; response commit binding belongs to the old call. Reopening the original Vault finds the successfully saved pending candidate.
- `closing_keeps_progress_until_the_worker_has_joined`: all four public status/task commands remain callable while shutdown joins a channel-controlled worker. Task history clears after join.
- `poisoned_lifecycle_refuses_work_but_allows_observation_and_shutdown`: synthetic gate poison, opaque non-retryable refusal, observation and cleanup.

Repository-root checks passed on pinned Rust 1.98.1 GNU, offline, with MinGW ahead of Rust in `PATH`: format check; **222 workspace tests** (including **20 Core tests**); Clippy with warnings denied; independent Python 3.12 / python-jsonschema 4.26.0 cross-check of **34 schemas**; and recorded/logged Runtime surface aggregate `34ce4f550c7d23909f8794b8a1128bff15a64e127841498d3a3043d12f79e8f8`. The Rust suite ran outside the sandbox so the existing Windows ACL tests could execute. `git diff --check` passed. Logs remain local and ignored.

## Limits

All evidence is synthetic and local. Runtime pin adoption, its adapter/UI concurrency, actual Windows application/installer suites, real exports, Provider calls, signing, Narrator, OS crashes and power-loss durability remain unverified. Native tray/page exit still uses the reference shell's existing scheduling; Core protection does not establish UI responsiveness for those paths.
