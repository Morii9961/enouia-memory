# MV-6 follow-up — installer, startup, accessibility

## Current result — 2026-10-03

The owner authorized continued MV-6 work and a separate commit for each completed feature. The final integrated real-app run passed **43/43** synthetic checks. The final **7.04 MiB NSIS installer** was rebuilt successfully after a test-process file lock was resolved, then passed **8/8** isolated installation/uninstallation checks. Required root checks passed: pinned offline format checking, **196 Rust workspace tests**, Clippy with warnings denied, and independent Python 3.12 / python-jsonschema 4.26.0 validation of **34 schemas**, including 4,913 set records and 63 workspace messages. Frontend and separate test-fixture typechecks/builds passed.

| Completed feature | Evidence | Commit |
|---|---|---|
| Harness readiness, enabled-control waiting and failure cleanup | [Harness](MV-6-harness.md) | `518dbce` |
| Current-user installer and reversible installation drill | [Installation](MV-6-installation.md) | `eaf893e` |
| Explicit opt-in startup, initially hidden main window, no automatic Vault, scoped settings | [Startup](MV-6-startup.md) | `9b581bc` |
| Original write key/action retained on retry and serial operation polling | [Retry](MV-6-retry.md) | `d0347d9` |
| Modal keyboard focus, origin restoration, review completion focus and responsive/forced-color styles | [Accessibility](MV-6-accessibility.md) | `7148a44` |
| Actual renderer crash, forced owned-app exit and acknowledged-record recovery | [Crash](MV-6-crash.md) | `68d1c3c` |

The initial injection failed because Tauri's invocation property is immutable; a separately built fixture now uses the production hook/client and real Core without a production fault API. The helper navigation race and clicks on disabled buttons were fixed. Native probing now distinguishes the main window from a tray helper. Keyboard failures were reproduced and fixed by containing modal focus, capturing its origin before asynchronous preparation disables the button, and focusing the review heading after the candidate is removed. All of these cases pass in the final run; the October 2 pending statements below are historical.

**Still pending:** actual Windows login startup, Narrator, an actual Windows contrast theme, interactive installer accessibility, upgrade from an earlier installed version, and code signing. Browser accessibility-tree/forced-color checks and isolated registry tests do not establish those results. Manual accessibility steps are in the accessibility report. Crash evidence covers acknowledged synthetic records after a renderer crash and forced app termination, not an interrupted commit, OS crash, power loss, disk failure or real-data recovery.

All Vaults, logs, screenshots, builds and fixture bundles remain ignored local/temporary material. No real startup Run entry was enabled. The production frontend contains neither the fault marker nor its test API. No Runtime dependency, Activity reader, real Provider or model request was introduced. **MV-7 has not started** and requires a separate owner request. Other limits in the original merged stage report remain unchanged.

## Historical 2026-10-02 snapshot — superseded by the result above

Date: 2026-10-02. Base: merged `3523ca8`; branch `codex/mv6-install-accessibility`. The owner authorized continued work until the five-hour quota has about 5% remaining. This work stays inside MV-6. MV-7 has not started. All Vault tests use synthetic data in temporary directories.

## Changes

- Added pinned Tauri CLI 2.12.0 and a current-user NSIS installer. Uninstall removes the startup value only when it exactly matches this installation's quoted executable and `--autostart` arguments. Vaults and backups are not installer-owned.
- Added explicit opt-in Windows login startup. Only the fixed current-user Run value is exposed to the main window; the page cannot choose registry paths, executables, arguments, or Vault paths. A value from a different installation is preserved. Startup hides the main window and does not automatically open a Vault.
- UI retries retain the original write idempotency key. New submissions receive a new key. Operation polling is serial, displays failures, supports retry, and stops on unmount.
- Added dialog initial focus and focus restoration, busy-state protection, status announcements, visible focus, responsive layout, and forced-colors styles. Keyboard navigation does not switch pages while a modal is open.
- Corrected README stage and explicit-data-root descriptions.

## Verified

| Check | Result |
|---|---|
| Pinned offline `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | 196 passed, 0 failed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| Python 3.12 / python-jsonschema 4.26.0 independent cross-check | 34 schemas, 4,913 set records, 63 workspace messages passed |
| Frontend typecheck and Vite production build | passed |
| `npm run bundle` | release executable and NSIS installer generated, about 7.04 MiB |
| Startup unit tests | 3 passed, including quoted-command validation and isolated registry round-trip |

The registry test uses an isolated non-startup test key, then removes it. It does not enable the real Windows Run entry. Local logs and build artifacts are ignored; no Vault, database, export, or credentials are included in this change.

## Pending and failed validation attempts

The original merged MV-6 real-app acceptance remains 22/22. It does not validate the new changes. A follow-up run reached import and candidate creation, but its attempted lost-success-response injection did not take effect and timed out. That ineffective injection was removed rather than weakening production IPC protections. A subsequent run failed in harness initialization with `ReferenceError: __t is not defined` before acceptance checks. The cause has not yet been established. Consequently, neither retry-after-lost-response nor the added dialog accessibility and forced-colors checks has an end-to-end pass.

Next work should isolate the harness lifecycle/loopback-debugging state and rerun it against a fresh synthetic Vault. Retain a separate deliberate lost-response test without weakening production IPC. Then check real keyboard focus restoration, Narrator, and Windows high contrast. Automated accessibility-tree and forced-colors checks alone would not establish manual screen-reader acceptance.

The generated installer has not been installed or uninstalled in this follow-up. Actual opt-in startup at Windows login, uninstall startup cleanup, app-file replacement, and code signing remain unverified. On machines missing WebView2, the default installer may download Microsoft's runtime; this is installation behavior, not a Memory model request. The installer and release executable predate a semantics-equivalent Clippy cleanup in registry length validation; rebuild before release acceptance.

## Sources

The packaging configuration follows [Tauri Windows installers](https://v2.tauri.app/distribute/windows-installer/). Startup command bounds follow [Microsoft Run and RunOnce registry keys](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys); the bounded registry read uses [RegGetValueW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-reggetvaluew). No Runtime dependency or Activity reader was added.
