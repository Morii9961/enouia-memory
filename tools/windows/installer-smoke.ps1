param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$BuiltExecutable
)
# Reversible synthetic installation drill. Refuse an existing installation;
# never remove or replace its registry entries, shortcuts, or app files.
$ErrorActionPreference = 'Stop'
$product = 'Enouia Memory Workspace'
$productKey = "HKCU:\Software\enouia\$product"
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$product"
foreach ($key in @($productKey, $uninstallKey,
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$product",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$product")) {
    if (Test-Path -LiteralPath $key) { throw 'Existing installation metadata: drill refused.' }
}
$run = Get-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
foreach ($name in @($product, 'EnouiaMemoryWorkspace')) {
    if ($null -ne $run.GetValue($name, $null)) { throw 'Existing startup entry: drill refused.' }
}
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$executablePath = (Resolve-Path -LiteralPath $BuiltExecutable).Path
# Tauri 2.12 patches this one bundle marker for the installed payload, then
# restores the build output. Compare every byte after that exact transform.
# Source: tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle.rs, patch_binary.
$encoding = [Text.Encoding]::GetEncoding(28591)
$originalBytes = [IO.File]::ReadAllBytes($executablePath)
$originalText = $encoding.GetString($originalBytes)
$marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
$offset = $originalText.IndexOf($marker, [StringComparison]::Ordinal)
if ($offset -lt 0 -or $originalText.IndexOf($marker, $offset + $marker.Length, [StringComparison]::Ordinal) -ge 0) { throw 'Expected one unpatched Tauri bundle marker.' }
$expectedBytes = $encoding.GetBytes($originalText.Replace($marker, '__TAURI_BUNDLE_TYPE_VAR_NSS'))
$sha = [Security.Cryptography.SHA256]::Create()
$expectedHash = [BitConverter]::ToString($sha.ComputeHash($expectedBytes)).Replace('-', '')
$sha.Dispose()
$root = Join-Path ([IO.Path]::GetTempPath()) ('enouia-installer-' + [guid]::NewGuid().ToString('N'))
$installDir = Join-Path $root 'Synthetic install'
$sentinel = Join-Path $root 'Synthetic Vault.txt'
New-Item -ItemType Directory -Path $root | Out-Null
Set-Content -LiteralPath $sentinel -Value 'Synthetic user-selected data outside the app installation.'
$sentinelHash = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash
$checks = [ordered]@{}
$didInstall = $false
try {
    # /NS suppresses shortcuts. /D must be last; NSIS parses its remainder
    # as the directory, including spaces. /R is deliberately absent.
    $process = Start-Process -FilePath $installerPath -ArgumentList "/S /NS /D=$installDir" -WindowStyle Hidden -PassThru -Wait
    $didInstall = Test-Path -LiteralPath (Join-Path $installDir 'uninstall.exe')
    if ($process.ExitCode -ne 0 -or -not $didInstall) { throw 'Installer did not complete.' }
    $checks.install_exit_zero = $true
    $installedExe = Join-Path $installDir 'enouia-memory-workspace.exe'
    $checks.installed_executable_matches = (Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash -eq $expectedHash
    $registered = (Get-ItemProperty -LiteralPath $uninstallKey).InstallLocation
    $checks.explicit_install_directory = $registered.Trim('"') -eq $installDir
    $checks.startup_not_enabled = $null -eq $run.GetValue('EnouiaMemoryWorkspace', $null)
} finally {
    if ($didInstall) {
        # Resolve and check the owned path before executing its uninstaller.
        $ownedUninstaller = (Resolve-Path -LiteralPath (Join-Path $installDir 'uninstall.exe')).Path
        if (-not $ownedUninstaller.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Uninstaller escaped the test root.' }
        $process = Start-Process -FilePath $ownedUninstaller -ArgumentList "/S _?=$installDir" -WindowStyle Hidden -PassThru -Wait
        $checks.uninstall_exit_zero = $process.ExitCode -eq 0
        $checks.app_removed = -not (Test-Path -LiteralPath (Join-Path $installDir 'enouia-memory-workspace.exe'))
        $checks.registration_removed = -not (Test-Path -LiteralPath $uninstallKey)
    }
    $checks.external_synthetic_data_preserved = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash -eq $sentinelHash
    # NSIS retains install-location/language preferences by default. Remove
    # this drill's new metadata only if it still points at our test root.
    if (Test-Path -LiteralPath $productKey) {
        $location = (Get-Item -LiteralPath $productKey).GetValue('')
        if ($location -eq $installDir) { Remove-Item -LiteralPath $productKey }
    }
    $checks | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'report.json')
    foreach ($name in $checks.Keys) { Write-Output "$name=$($checks[$name])" }
    Write-Output "Synthetic evidence: $root"
}
if ($checks.Count -ne 8 -or $checks.Values -contains $false) { throw 'Installer drill failed.' }
