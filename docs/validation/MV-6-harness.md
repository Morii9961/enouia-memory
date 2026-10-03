# MV-6 acceptance harness follow-up

Date: 2026-10-03. Scope: synthetic real-app acceptance, following the unsuccessful 2026-10-02 runs.

The previous helper initialization could run before the initial page navigation finished. The harness now waits for the document and rendered React root before returning a connection. It also waits for matching buttons to become enabled: `session_new` was previously clicked while the initial session list was loading, so the disabled button did nothing. All test-created app processes and debugging sessions are closed in the final cleanup, including failure paths.

Diagnostics established that Tauri's internal `invoke` property is non-writable and non-configurable. Assigning a replacement did not intercept calls. The production IPC configuration was not weakened to make that injection work.

The corrected harness completed **25/25 checks** against the release app and a new synthetic Vault. These include the original 22 checks plus plan initial focus, dialog accessibility-tree name, and browser-emulated forced colors. They do not constitute Narrator or actual Windows high-contrast acceptance, and they do not yet cover lost-response retry. Local diagnostic logs, screenshots, reports, and Vaults are ignored and not committed.

Command: `node apps/workspace/e2e/smoke.mjs <release-exe> <synthetic-vault> <synthetic-markdown> <temporary-output> K`. Exit 0. Format/diff checking of this harness change passed. Rust checks for the surrounding implementation are recorded separately in [MV-6 follow-up](MV-6-followup.md).
