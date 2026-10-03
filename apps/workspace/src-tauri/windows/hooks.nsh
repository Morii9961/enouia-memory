; This installer owns app files, never a user-selected Vault or backup.
; Startup is opt-in inside the app. Uninstall removes only its exact command.
!ifndef ENOUIA_STARTUP_RUN_KEY
  !define ENOUIA_STARTUP_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!endif
!macro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $R0 HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
  StrCmpS $R0 '$\"$INSTDIR\enouia-memory-workspace.exe$\" --autostart' 0 +2
    DeleteRegValue HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
!macroend
