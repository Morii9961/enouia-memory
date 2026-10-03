param([Parameter(Mandatory = $true)][string]$MakeNsis)
# Exercise the actual uninstall macro on an isolated non-startup registry
# key. Production uses its fixed Run key; no real login entry is changed.
$ErrorActionPreference = 'Stop'
$tool = (Resolve-Path -LiteralPath $MakeNsis).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('enouia-startup-cleanup-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$testKey = 'Software\EnouiaMemoryTests\Cleanup-' + [guid]::NewGuid().ToString('N')
$hook = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\apps\workspace\src-tauri\windows\hooks.nsh')).Path
$source = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
OutFile "@OUTPUT@"
!define ENOUIA_STARTUP_RUN_KEY "@KEY@"
!include "@HOOK@"
Section
  StrCpy $INSTDIR "C:\Synthetic install"
  WriteRegStr HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace" '$\"$INSTDIR\enouia-memory-workspace.exe$\" --autostart'
  !insertmacro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ReadRegStr $R1 HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
  IfErrors exact_removed failed
exact_removed:
  WriteRegStr HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace" '"C:\Synthetic other\memory.exe" --other'
  !insertmacro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $R1 HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
  StrCmpS $R1 '"C:\Synthetic other\memory.exe" --other' foreign_preserved failed
foreign_preserved:
  WriteRegStr HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace" '"c:\synthetic install\enouia-memory-workspace.exe" --autostart'
  !insertmacro NSIS_HOOK_PREUNINSTALL
  ReadRegStr $R1 HKCU "${ENOUIA_STARTUP_RUN_KEY}" "EnouiaMemoryWorkspace"
  StrCmpS $R1 '"c:\synthetic install\enouia-memory-workspace.exe" --autostart' passed failed
passed:
  SetErrorLevel 0
  Goto cleanup
failed:
  SetErrorLevel 1
cleanup:
  DeleteRegKey HKCU "${ENOUIA_STARTUP_RUN_KEY}"
SectionEnd
'@
$output = Join-Path $root 'startup-cleanup.exe'
$source = $source.Replace('@OUTPUT@', $output).Replace('@KEY@', $testKey).Replace('@HOOK@', $hook)
$script = Join-Path $root 'startup-cleanup.nsi'
Set-Content -LiteralPath $script -Value $source -Encoding UTF8
& $tool /V2 $script
if ($LASTEXITCODE -ne 0) { throw 'NSIS test compile failed.' }
$process = Start-Process -FilePath $output -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0) { throw 'Startup cleanup macro failed.' }
if (Test-Path -LiteralPath ('HKCU:\' + $testKey)) { throw 'Isolated test key was not removed.' }
Write-Output '3/3 exact-match, foreign-value and case-sensitive startup cleanup checks passed; isolated key removed.'
