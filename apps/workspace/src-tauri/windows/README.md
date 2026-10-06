# Installer ownership boundary

`installer.nsi` is derived from the MIT-licensed [Tauri CLI 2.12.0 template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi). The unmodified downloaded source has SHA-256 `DABED59013B1D78B879A1A85BC7F2EED2993B33A9A90CDABE5946DE3D3950597`. Its license is retained in `installer-LICENSE-MIT.txt`.

Two sections are removed, with no other upstream behavior changes:

- The uninstall confirmation page's “delete app data” checkbox and callbacks. Vault locations are explicitly chosen and may be inside a directory the generic template considers application data.
- The uninstall section's generic product-name Run-value deletion and recursive AppData deletion. `hooks.nsh` alone clears the fixed startup value after an exact command match. Like upstream's deletion, it runs at the end of the uninstall section, after the running-app check, and not in update mode, so a cancelled uninstall keeps the value. Uninstall removes application files; Memory deletion remains a Core review operation.

The preinstall hook also checks registered versions before file replacement; see the [upgrade report](../../../../docs/validation/MV-6-upgrade.md). Upstream's own installer/uninstaller pages, application file handling, installation registration, shortcuts, language support, and WebView2 installation remain in place.

Before updating the pinned CLI or this template, check both removals, run `tools/windows/installer-template-check.ps1`, build the actual installer, and rerun installation, upgrade and exact startup cleanup drills. Keep generated paths, local registry data, and built installers out of Git.
