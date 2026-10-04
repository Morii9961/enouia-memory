# MV-6 Windows installation follow-up

Date: 2026-10-03. Scope: current-user NSIS packaging and a reversible installation drill. No real Vault or login startup entry was created. MV-7 remains unstarted.

Additional 2026-10-04 evidence: the [upgrade and downgrade report](MV-6-upgrade.md) adds 19/19 real-package checks and 7/7 version guard checks, including a correction for silent downgrade in the pinned template. Interactive upgrade and earlier data-format migration remain pending.

Pinned `@tauri-apps/cli` 2.12.0 builds the frontend, release application, and a 7.04 MiB NSIS installer with `npm run bundle` in `apps/workspace`. The installer targets the current user, permits Simplified Chinese and English, and disables downgrades. The custom uninstall hook removes the fixed `EnouiaMemoryWorkspace` startup value only when its command exactly belongs to the installation being removed.

`tools/windows/installer-smoke.ps1` refuses existing installation metadata or startup entries, installs silently into a fresh temporary directory containing spaces, suppresses shortcuts, and uninstalls from that verified owned path. **8/8 checks passed**: installation exit status, installed payload identity, requested installation directory, no opt-in startup enabled, uninstall exit status, app removal, uninstall registration removal, and preservation of a synthetic data sentinel outside the app directory. The drill removes its newly created install-location preference only when it still matches the test directory. Its local reports and synthetic data are not committed.

The payload comparison accounts for exactly one known Tauri transformation: the bundle marker is changed from `UNK` to `NSS` for the NSIS payload, and the build output is restored afterward. Every other byte must match. This behavior is verified against the [pinned CLI source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle.rs), rather than accepting an arbitrary hash difference.

The surrounding source passed 196 offline Rust workspace tests, format checking, Clippy with warnings denied, the independent 34-schema cross-check, and frontend typechecking/build; see [follow-up](MV-6-followup.md). Build and drill output is retained locally under ignored material.

Limits: this is an unsigned development installer. The drill does not establish interactive installer accessibility, actual Windows login behavior, upgrade from an earlier installed version, or crash/power-loss safety. WebView2 was already available on this machine; the default installer may download Microsoft's runtime on a machine missing it. Packaging follows [Tauri's Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/).
