# MV-6 opt-in startup follow-up

Date: 2026-10-03. Scope: the app's fixed current-user startup setting and its background launch mode. No real Windows login startup entry was enabled and no real Vault was opened.

The main window exposes an explicit checkbox, initially off. Only its capability allows `startup_status` and `startup_set`. The shell writes a quoted current executable followed by `--autostart`; registry names, executable paths, arbitrary arguments and Vault roots cannot be supplied by the page. Unsupported, invalid, oversized or foreign entries fail without disclosing their paths. A different installation's value is preserved.

Background mode applies `visible = false` before creating the main native window, avoiding a show-then-hide transition. The tray can subsequently show it. No Vault is automatically opened.

Verified with synthetic data:

- All required offline root checks passed: formatting, **196 workspace tests**, Clippy with warnings denied, and the independent **34-schema** cross-check.
- Three Rust startup tests passed, including a registry round-trip on an isolated non-startup key.
- Five targeted real-app checks passed: the titled native main window starts hidden, Vault state is `none`, startup status is readable from the main window, overlay startup writes are denied by the ACL, and the main window can be shown afterward. Native probing distinguishes the main window from the tray's untitled auxiliary window.
- The actual NSIS uninstall macro passed **3/3** isolated tests: remove the exact installation command, preserve a foreign command, and preserve a case-different command. Its generated non-startup test key was removed. Production retains the fixed Run key; the test override is a compile-time NSIS definition, never a UI argument.

The remaining validation is actual opt-in startup at a Windows login, and interactive settings/installer accessibility. The native hidden-window test and isolated registry tests do not establish that acceptance. The hook uses the [case-sensitive NSIS comparison](https://nsis.sourceforge.io/Reference/StrCmpS); command length follows [Microsoft Run key guidance](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys).
