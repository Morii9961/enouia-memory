param([Parameter(Mandatory = $true)][string]$MakeNsis)
# A file the installer cannot replace must fail a silent install, not be
# skipped. Tiny installers copy over a locked synthetic file: one includes
# the production hooks (AllowSkipFiles off), the control uses NSIS's default.
# Nothing is installed or registered.
$ErrorActionPreference = 'Stop'
$tool = (Resolve-Path -LiteralPath $MakeNsis).Path
$hook = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\apps\workspace\src-tauri\windows\hooks.nsh')).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('enouia-locked-file-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$payload = Join-Path $root 'payload.txt'
Set-Content -LiteralPath $payload -Value 'Synthetic new payload'
$checks = [ordered]@{}
foreach ($case in @(@{ name = 'production_hooks'; include = "!include `"$hook`"" }, @{ name = 'nsis_default'; include = '' })) {
    $target = Join-Path $root $case.name
    New-Item -ItemType Directory -Path $target | Out-Null
    $existing = Join-Path $target 'payload.txt'
    Set-Content -LiteralPath $existing -Value 'Synthetic old payload'
    $after = Join-Path $target 'after.txt'
    $script = Join-Path $root "$($case.name).nsi"
    $output = Join-Path $root "$($case.name).exe"
    @"
Unicode true
RequestExecutionLevel user
SilentInstall silent
$($case.include)
OutFile "$output"
Section
  SetOutPath "$target"
  File "$payload"
  FileOpen `$0 "$after" w
  FileClose `$0
SectionEnd
"@ | Set-Content -LiteralPath $script -Encoding UTF8
    & $tool /V2 $script
    if ($LASTEXITCODE -ne 0) { throw 'Locked-file test compile failed.' }
    $lock = [IO.File]::Open($existing, 'Open', 'Read', 'None')
    try {
        $process = Start-Process -FilePath $output -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Locked-file installer did not finish.' }
    } finally { $lock.Dispose() }
    $kept = (Get-Content -LiteralPath $existing) -ceq 'Synthetic old payload'
    $checks[$case.name] = if ($case.name -eq 'production_hooks') {
        $process.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $after) -and $kept
    } else {
        # The control shows what the hooks prevent: exit 0 with the file skipped.
        $process.ExitCode -eq 0 -and (Test-Path -LiteralPath $after) -and $kept
    }
}
foreach ($name in $checks.Keys) { Write-Output "$name=$($checks[$name])" }
Write-Output "Synthetic evidence: $root"
if ($checks.Count -ne 2 -or $checks.Values -contains $false) { throw 'Locked-file checks failed.' }
