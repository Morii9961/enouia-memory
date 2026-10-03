; This installer owns app files, never a user-selected Vault or backup.
; Startup is opt-in inside the app. Uninstall removes only its exact command.
!macro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "EnouiaMemoryWorkspace"
  StrCmp $R0 '$\"$INSTDIR\enouia-memory-workspace.exe$\" --autostart' 0 +2
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "EnouiaMemoryWorkspace"
!macroend
