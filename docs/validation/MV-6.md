# MV-6 — Windows Memory Workspace (synthetic)

Date: 2026-10-02. Scope: MV-6 ([plan §10](../design/IMPLEMENTATION_PLAN.md)), authorized by the owner's "继续吧" after the MV-5 status report, and finished on the owner's request to complete MV-6 and open a pull request. Start: `ae6a6d0`. Every input is synthetic: a Vault created in a temporary directory, a generated Markdown file, and typed test sentences. No real data root, export, Provider key, model, network, MCP, or VPS was used. MV-7 has not started.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| Design (ADR-MEM-44) | Embedded Core behind one typed channel; Tauri shell only forwards; picker tokens instead of paths; trusted surface `trusted_windows_app`; long work as observable operations; four distinct stops. | `docs/adr/README.md` |
| Workspace IPC v1 | 36 commands with strict arguments, idempotency keys on every command that can commit, command/response pairing, error envelope `{code, retryable, rules}`. Fixtures are checked by the Rust subset validator and python-jsonschema. | `contracts/ipc/workspace-v1.schema.json`, `enouia-memory-contract::workspace`, `tests/fixtures/memory/workspace-manifest.json` |
| MV-6.1 Core and pages | `enouia-memory-workspace`: explicit root only (`--vault` or folder picker; a new Vault needs the typed phrase), review plans confirmed by the shown diff hash, remember and correction as pending candidates, forget/purge through the same plan, imports with preview, progress, cancel and resume, paging bound to the snapshot. Pages: Memory Explorer, Import Center, Candidate Review, Sessions. | `crates/enouia-memory-workspace`, `apps/workspace/src` |
| MV-6.2 Context Inspector | Compile preview vs. dispatched request (`preview_not_sent` / `dispatched` with prepared, sent, completed times), every inclusion/exclusion with its reason, the actual request re-rendered from the saved records and checked against the dispatch hash. | `context_inspect`, `dispatch_inspect` |
| MV-6.3 Recovery / Status | Index rebuild (cancellable, `Index::rebuild_with`), Vault verification, backup export to an empty folder, restore preview of an export, lock/unlock, component health (`core`, `vault`, `memory_index`, `provider`, `sync`, `activity`), free space, owner-only ACL, delete-impact preview. | `workspace_status`, Vault & Recovery and Status pages |
| MV-6.4 Companion shell | Close hides, tray (show, lock, exit), global hotkey `Ctrl+Alt+<letter>` (`M` by default, `--hotkey-key` to change) registered with `RegisterHotKey` and reported as `registered` or `conflict`, always-on-top quick-search overlay. | `apps/workspace/src-tauri` |
| Fix found on the way | The CLI `purge` reused the plan nonce as the file-purge key under the scope the confirm commit had just used, so every CLI purge ended in `idempotency_conflict` after the tombstone. The file purge now has its own key; a CLI regression test covers it. | `enouia-memory-cli` |

Toolchain: Tauri 2.12.0 / tauri-build 2.7.0, rfd 0.17.2, windows-sys 0.61.2 (Rust, offline from the local registry cache); React 19.3.0, Vite 8.3.1, TypeScript 7.0.2, @tauri-apps/api 2.12.0 (`npm install --offline`). The shell builds in a clean checkout without the frontend (dev configuration); the shipped binary embeds `dist/` with `--features custom-protocol` after `npm run build`.

## 2. Acceptance items

Real-app evidence comes from `apps/workspace/e2e/smoke.mjs`: it starts the release binary with WebView2 remote debugging on loopback, drives the real page over the DevTools protocol (real Tauri IPC, real Core), fills the native Open dialog of that process through UI Automation and Win32 messages, and writes `report.json` plus ten screenshots. Final run: **22/22 checks passed**.

| Item | Status | Evidence |
|---|---|---|
| W01 full task without engineering tools | Passed | e2e: import through the native dialog (`W01.import`), candidate → plan with the exact text and an 8-hex code → confirm (`W01.plan_shows_exact_text`), memory and source excerpt (`W01.source_visible`), correction → revise candidate showing the old fact → confirm (`W01.correct`), ask in a new session with sources (`W01.answer_with_sources`), inspect the actual request (`W01.inspect_actual_request`), exit, restart, and the transcript is there (`W01.resume_after_restart`). Core: `w01_import_review_ask_inspect_correct_and_resume`. |
| W02 errors and long work | Passed | e2e: a status call during index rebuild returns in milliseconds (`W02.ui_not_blocked_during_rebuild`). Core `w02_*`: paging and stale cursors, `index_not_ready` at once while the index is held (status `memory_index: recovering`, Vault `healthy`), cancellation reported as `cancelled` never `succeeded`, verify and backup as operations, restore preview of that export. Pages show errors with their code and rules and offer retry only when `retryable`; write retries reuse their idempotency key. |
| W03 close / exit / lock / pause | Passed | e2e: closing the main window hides it while the Core keeps answering (`W03.close_hides_core_keeps_running`), exit ends the process (`W03.exit_ends_process`), lock refuses work until unlock (`W03.lock_refuses`), the four stops are explained (`W03.four_stops_explained`); sync does not exist before MV-9 and is shown as unavailable. Activity is shown as `independent_not_managed` and never touched. Core `w03_*`: a plan made before a lock cannot be confirmed after it. Writes still go through the Vault's per-commit writer lock, so the UI and the CLI cannot double-write. |
| W04 frontend capabilities and rendering | Passed | e2e: `plugin:fs` and `plugin:shell` are denied by the ACL, `fetch` to a remote host is blocked by the CSP, a path in place of a token is refused (`workspace.token`), the page sees only file names, and a memory containing `<img onerror>` / `<script>` renders as text with no element or script run. Core `w04_*`: unknown commands, extra fields, other schema versions, and tokens of another kind are refused without echoing paths. |
| W05 accessibility and installation behavior | Partly passed | e2e: `Alt+2` switches page and moves focus to its heading (`W05.keyboard_page_switch`); the hotkey registers (`W05.hotkey_registered`) and a second instance reports the conflict instead of failing silently (`W05.hotkey_conflict_reported`). On this machine the default `Ctrl+Alt+M` is already taken by another program; the app reports that as a conflict, and the run used `Ctrl+Alt+K`. The app writes no log files. **Pending:** no installer or autostart exists yet (`bundle.active = false`); screen-reader and high-contrast checks were not run. |

## 3. Checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | 193 passed, 0 failed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 (with and without the built frontend) |
| `python tools/schema-check/check_schemas.py` | OK (34 schemas; 63 workspace IPC messages added) |
| `npm run build` in `apps/workspace` | typecheck and Vite build pass |
| `node apps/workspace/e2e/smoke.mjs …` | 22/22 checks passed |

## 4. Limits and open items

- The owner boundary is still in-process: whoever runs the app on this Windows account is the owner. MV-8's Host must authenticate callers.
- "Last successful backup" is known only for exports made by the running process; the Vault does not record backups, and "last verified restore" is not recorded. Actual restore stays in the CLI (`restore` into an empty folder); the UI only previews an export.
- Search is MV-4's literal search: a natural question such as "MoriMeta 的设计决定是什么？" does not match, while "MoriMeta 设计决定" does. The Mock then answers `no_supported_evidence`, as designed.
- Import progress counts cancellation checks between batches, not units; total is unknown.
- No installer, autostart, or code signing; no screen-reader pass. The last-used data root is not remembered (no default location by design).
- Recovery: every MV-6 change is additive (new crates, new contract, a new app directory) except the CLI purge key and `Index::rebuild_with`; reverting the branch restores MV-5 behavior.

## 5. Next stage input

MV-7 (real Provider and optional extraction) needs the owner's choice of Provider(s), an API key stored in the OS secret store, usage and cost limits, and explicit permission for each outgoing request class. It does not start without the owner's request.
