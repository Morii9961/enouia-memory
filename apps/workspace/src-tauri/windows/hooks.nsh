; This installer owns app files, never a user-selected Vault or backup.
; Startup is opt-in inside the app. Uninstall removes only its exact command,
; and only once the app files are gone: after the running-app check (which a
; Cancel aborts) and never in update mode, as upstream.
!ifndef ENOUIA_STARTUP_RUN_KEY
  !define ENOUIA_STARTUP_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!endif
; Tauri CLI 2.12.0's silent downgrade check reads $R0 populated by the
; reinstall page, which /S skips. Compare the registered version afresh
; before copying any application files; do not depend on page state.
!macro NSIS_HOOK_PREINSTALL
  Push $R0
  Push $R1
  ReadRegStr $R0 HKCU "${UNINSTKEY}" "DisplayVersion"
  ${If} $R0 != ""
    ; The pinned plugin treats malformed versions as older than valid ones.
    ; A known-invalid first argument returns -1 only if the second is valid.
    nsis_tauri_utils::SemverCompare "enouia-invalid-version" $R0
    Pop $R1
    ${If} $R1 != -1
      SetErrorLevel 1
      Abort "The installed version is unrecognized and must not be replaced."
    ${EndIf}
    nsis_tauri_utils::SemverCompare "${VERSION}" $R0
    Pop $R1
    ${If} $R1 != 0
    ${AndIf} $R1 != 1
      SetErrorLevel 1
      Abort "A newer or unrecognized installed version must not be replaced."
    ${EndIf}
  ${EndIf}
  Pop $R1
  Pop $R0
!macroend
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ReadRegStr $R0 HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
    StrCmpS $R0 '$\"$INSTDIR\enouia-memory-workspace.exe$\" --autostart' 0 +2
      DeleteRegValue HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
  ${EndIf}
!macroend
