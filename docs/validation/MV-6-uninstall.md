# MV-6 uninstall ownership boundary

Date: 2026-10-04. Base: `518364e`. Scope: current-user installer behavior. No real Vault or real startup value was used.

The stock pinned Tauri CLI 2.12.0 template offers a “delete app data” checkbox and, when selected, recursively removes the bundle identifier's roaming and local AppData directories. Memory opens only user-selected Vault locations and does not reserve those paths as disposable cache. The same stock section deletes a product-name Run value without comparing its command. Both paths conflict with ADR-MEM-44's application-file-only installer boundary.

The configured `windows/installer.nsi` is a local, MIT-licensed copy of the [pinned upstream template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), with precisely two removed sections: the AppData checkbox/callbacks and the generic AppData/Run-value deletion block. The source hash, license and changes are documented alongside it. Application files, installation registration, shortcuts, WebView2, language selection, and installer/uninstaller pages retain their upstream behavior. The existing exact-command startup cleanup and preinstall downgrade guard remain in `hooks.nsh`. Removing memories remains a Core review operation.

## Verified

- The static ownership check passed **10/10** checks on the configured source and **14/14** when also checking the generated installer script. It checks that both unwanted deletion branches and the checkbox are absent, the fixed CLI version matches the template, the license is present, and application removal and the scoped hooks remain.
- The same check rejected the original upstream template's checkbox, recursive AppData deletion, and unconditional Run-value deletion. This establishes that the check detects the original paths.
- The production 0.1.0 and test-only 0.1.1 packages compiled successfully with the local template. The rebuilt packages passed the **19/19** upgrade/Vault preservation checks again, and the production package passed the **8/8** fresh installation/uninstallation checks.
- The actual startup cleanup macro passed **3/3** exact-match, foreign-value and case-sensitive checks in an isolated non-startup key, which was removed afterward.

The current-day 196 offline Rust tests, format check, Clippy and independent 34-schema cross-check remain applicable: this change alters packaging source only. Reports, packages, and synthetic Vaults are ignored local material. The final executable remains the verified 0.1.0 build; no production version or provider behavior changed.

## Limits

The AppData deletion issue was established from the pinned source and by the negative ownership check; no real AppData folder was deleted to reproduce it. Silent real-package drills verify application cleanup and synthetic data preservation, while removal of the interactive checkbox/deletion branch is established by source and generated-script checks. This is not Narrator or interactive installer accessibility acceptance. Previously distributed uninstallers retain their old behavior. Actual login, manual accessibility/upgrade, signing, earlier data-format migration and other stage gaps remain pending. MV-7 has not started.
