# MV-6 quick-search permissions and read lifecycle

Date: 2026-10-04. Base: `546533a`. Scope: MV-6 native window permissions and quick-search UI. MV-7 remains unstarted.

## Reproduced failures

The actual release application's overlay could call `remember` through `workspace_call` and create a synthetic candidate. Its displayed read-only UI did not constrain the native channel. The overlay also shared the main window's file picker and exit capability. A separate deferred-client fixture reproduced stale results/errors, early busy clearing, empty-query requests, retained contents on reopening, and missing retry; the baseline passed **0/12** overlay checks.

## Changes

The native command uses Tauri's injected window identity. The main window retains its Core channel; the overlay may invoke only `memory_search`. Unknown windows and malformed/lookalike overlay commands are refused. Request fields named `window` or `principal` do not change the native identity. A separate overlay capability permits only the gated channel and showing/hiding windows, excluding picker, exit and startup settings.

Overlay reads publish only for their current generation. A new submitted query clears old results. Clearing or submitting an empty query, focus, blur and Escape invalidate pending reads and clear transient contents. Empty queries never call the Core. Visible errors support retry of the original submitted query. Main-window and tray lock hide the overlay; locked searches cannot publish a successful result. This does not change Memory IPC v1 or Core owner/review rules.

## Verified

- **36/36** deferred-client UI checks: 9 Explorer, 15 Sessions and 12 Overlay. These simulate responses and focus/blur events; they are not real hotkey or persistence evidence.
- **52/52** checks on the rebuilt actual app and a fresh synthetic Vault, including real overlay search, denial of remember/review/forget/lock with spoofed identity fields, unchanged candidate count after denials, picker/exit ACL denial, locked-search result clearing, startup denial, write retry and acknowledged-record recovery after an actual renderer crash.
- Native tests cover search-only scope, malformed commands, spoofed fields and unknown windows, alongside existing startup tests. Required pinned offline root checks passed: format, **201 Rust tests**, Clippy with warnings denied and Python 3.12 / python-jsonschema 4.26.0 validation of **34 schemas**. An initial sandboxed Windows ACL test failed; the identical focused test and full root suite passed with normal current-user filesystem permissions.
- Production frontend and separate fixture builds passed. Fixture markers/APIs are absent from the production frontend. The final 0.1.0 installer passed **8/8** isolated installation checks and **14/14** ownership-template checks.

The tests operate only owned app processes and synthetic temporary Vaults. One attempt to run both UI harnesses concurrently could not discover its separate WebView2 debug port; running the deferred harness after actual-app cleanup passed. No production permission was added to accommodate testing.

## Limits

This is local window/Core/Mock evidence. It does not establish hostile OS-user isolation, MV-8 Host authentication, real-provider behavior, manual Narrator/login/interactive upgrade acceptance, signing, or power-loss recovery. Those pending boundaries remain unchanged.
