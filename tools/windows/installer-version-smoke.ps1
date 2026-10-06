param(
    [Parameter(Mandatory = $true)][string]$MakeNsis,
    [Parameter(Mandatory = $true)][string]$TauriPlugins
)
# Execute the production preinstall macro against an isolated test key.
# These tiny installers neither register nor install the actual Workspace.
$ErrorActionPreference = 'Stop'
$compiler = (Resolve-Path -LiteralPath $MakeNsis).Path
$plugins = (Resolve-Path -LiteralPath $TauriPlugins).Path
$hook = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\apps\workspace\src-tauri\windows\hooks.nsh')).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('enouia-version-guard-' + [guid]::NewGuid().ToString('N'))
$parentName = 'Software\EnouiaMemoryTests'
$registryName = $parentName + '\Version-' + [guid]::NewGuid().ToString('N')
$registryPath = 'HKCU:\' + $registryName
New-Item -ItemType Directory -Path $root | Out-Null
New-Item -Path $registryPath -Force | Out-Null
$source = @'
Unicode true
RequestExecutionLevel user
SilentInstall silent
!include LogicLib.nsh
!addplugindir "@PLUGINS@"
!define VERSION "0.1.0"
!define UNINSTKEY "@KEY@"
!include "@HOOK@"
OutFile "@OUTPUT@"
Section
  StrCpy $R0 "preserved R0"
  StrCpy $R1 "preserved R1"
  !insertmacro NSIS_HOOK_PREINSTALL
  StrCmpS $R0 "preserved R0" 0 failed
  StrCmpS $R1 "preserved R1" 0 failed
  FileOpen $R2 "@MARKER@" w
  FileWrite $R2 "would install"
  FileClose $R2
  SetErrorLevel 0
  Goto done
failed:
  SetErrorLevel 3
done:
SectionEnd
'@
$output = Join-Path $root 'version-guard.exe'
$marker = Join-Path $root 'would-install.txt'
$script = Join-Path $root 'version-guard.nsi'
$source = $source.Replace('@PLUGINS@', $plugins).Replace('@KEY@', $registryName).
    Replace('@HOOK@', $hook).Replace('@OUTPUT@', $output).Replace('@MARKER@', $marker)
Set-Content -LiteralPath $script -Value $source -Encoding UTF8
$cases = @(
    @{ name = 'no_previous_version'; version = $null; allowed = $true },
    @{ name = 'upgrade'; version = '0.0.9'; allowed = $true },
    @{ name = 'same_version_reinstall'; version = '0.1.0'; allowed = $true },
    @{ name = 'downgrade'; version = '0.1.1'; allowed = $false },
    @{ name = 'malformed_version'; version = 'not-a-version'; allowed = $false },
    @{ name = 'previous_prerelease'; version = '0.1.0-beta.1'; allowed = $true },
    @{ name = 'newer_prerelease'; version = '0.1.1-beta.1'; allowed = $false }
)
$checks = [ordered]@{}
try {
    & $compiler /V2 $script
    if ($LASTEXITCODE -ne 0) { throw 'Version guard test compile failed.' }
    foreach ($case in $cases) {
        if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker }
        Remove-ItemProperty -LiteralPath $registryPath -Name DisplayVersion -ErrorAction SilentlyContinue
        if ($null -ne $case.version) {
            New-ItemProperty -LiteralPath $registryPath -Name DisplayVersion -PropertyType String -Value $case.version | Out-Null
        }
        $process = Start-Process -FilePath $output -WindowStyle Hidden -PassThru -Wait
        $installed = Test-Path -LiteralPath $marker
        $expectedExit = if ($case.allowed) { 0 } else { 1 }
        $retainedVersion = (Get-Item -LiteralPath $registryPath).GetValue('DisplayVersion', $null)
        $checks[$case.name] = $process.ExitCode -eq $expectedExit -and
            $installed -eq $case.allowed -and $retainedVersion -ceq $case.version
    }
} finally {
    # Exact fresh test key, no recursive deletion or real uninstall metadata;
    # then the parent only if nothing else uses it.
    Remove-Item -LiteralPath $registryPath
    $parent = Get-Item -LiteralPath ('HKCU:\' + $parentName)
    if ($parent.SubKeyCount -eq 0 -and $parent.ValueCount -eq 0) { Remove-Item -LiteralPath ('HKCU:\' + $parentName) }
    $checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'report.json')
    foreach ($name in $checks.Keys) { Write-Output "$name=$($checks[$name])" }
    Write-Output "Synthetic evidence: $root"
}
if ($checks.Count -ne 7 -or $checks.Values -contains $false) { throw 'Version guard checks failed.' }
