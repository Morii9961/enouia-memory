# MV-6 installation upgrade and downgrade protection

Date: 2026-10-04. Base: merged `d7b1500`. Scope: local MV-6 packaging and synthetic installation acceptance. MV-7 remains unstarted.

## Reproduced problem and correction

With the pinned Tauri CLI 2.12.0 and `allowDowngrades: false`, a silent installation of 0.1.0 over an installed synthetic 0.1.1 package returned success and replaced both the application payload and its registered version. The Vault remained intact. The pinned [installer template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi) checks a comparison stored by the reinstall page; silent mode skips that page.

The production preinstall hook now reads the current-user registered version and compares it immediately before application files are copied. It refuses newer or unrecognized installed versions with a nonzero exit. The pinned [version comparator](https://github.com/tauri-apps/nsis-tauri-utils/blob/nsis_tauri_utils-v0.5.3/crates/nsis-semvercompare/src/lib.rs) treats invalid versions as older than valid versions, so the hook independently checks validity with the same comparator before deciding whether installation may proceed. Temporary registers are restored on allowed paths. Dependencies and the production version remain unchanged.

## Verified

`tools/windows/installer-version-smoke.ps1` compiles and executes the actual production macro against a fresh non-startup test registry key. **7/7 checks passed**: no previous version, upgrade, same-version reinstall, downgrade refusal, malformed version refusal, upgrade from an earlier prerelease, and refusal of a newer prerelease. Allowed cases verify the macro restores its temporary registers. Every case verifies the registered version is untouched; refused cases write no simulated application file. The test key is removed afterward.

`tools/windows/installer-upgrade-smoke.ps1` passed **19/19 checks** against real unsigned NSIS packages. It creates a synthetic Vault, commits a confirmed synthetic source, verifies the Vault, installs 0.1.0 in a fresh temporary path containing spaces, and upgrades to a test-only 0.1.1 package without supplying a new installation directory. The installed payload matches the corresponding built executable byte for byte after the single known Tauri NSIS marker transform, and the original installation directory is retained. Attempting the rebuilt 0.1.0 installer then fails and leaves the newer payload and registration intact. Uninstallation removes the application and its registration while preserving every synthetic Vault file, its hash, and its acknowledged commit; final Vault verification passes. No real startup entry was enabled.

The drill refuses existing installation metadata, startup entries, or a running Workspace. Cleanup requires registration to still point at the drill's own installation and resolves the uninstaller inside its fresh test root. It does not recursively delete data. Reports and fixtures remain local and ignored.

Pinned offline format checking, **196 Rust workspace tests**, Clippy with warnings denied, and the independent Python 3.12 / python-jsonschema 4.26.0 **34-schema** cross-check passed. The frontend typecheck/build and both package builds passed. The final production executable was restored to its built 0.1.0 payload; test-only 0.1.1 is not a product release. The prior 43/43 real-app UI run remains evidence for unchanged UI/Core behavior, not a new installation acceptance claim.

## Limits

The two package versions are built from the same application source with a test-only version override. This proves package upgrade, file replacement, downgrade refusal, and preservation of acknowledged synthetic Vault records; it does not prove migration from an earlier application's data format or manual interactive upgrade acceptance. An already distributed installer without this hook cannot be retroactively made to refuse a downgrade. Actual Windows login startup, Narrator, a real system contrast theme, interactive installer accessibility, and signing remain pending. No real Vault, provider request, Runtime dependency, or Activity data was used.
