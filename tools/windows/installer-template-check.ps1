param([string]$RenderedTemplate)
# Static ownership checks complement actual installer drills. They also
# cover UI-only deletion branches which a silent uninstall cannot select.
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$shell = Join-Path $repo 'apps/workspace/src-tauri'
$config = Get-Content -LiteralPath (Join-Path $shell 'tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$package = Get-Content -LiteralPath (Join-Path $repo 'apps/workspace/package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$templatePath = (Resolve-Path -LiteralPath (Join-Path $shell $config.bundle.windows.nsis.template)).Path
$template = Get-Content -LiteralPath $templatePath -Raw -Encoding UTF8
$license = Get-Content -LiteralPath (Join-Path $shell 'windows/installer-LICENSE-MIT.txt') -Raw -Encoding UTF8
$checks = [ordered]@{
    configured_owned_template = $templatePath -ceq (Join-Path $shell 'windows\installer.nsi')
    pinned_cli_matches_template = $package.devDependencies.'@tauri-apps/cli' -ceq '2.12.0'
    no_appdata_delete_option = $template -notmatch 'DeleteAppDataCheckbox|\$\(deleteAppData\)'
    no_recursive_appdata_deletion = $template -notmatch '(?i)RmDir\s+/r\s+"\$(LOCALAPPDATA|APPDATA)\\'
    no_generic_startup_deletion = $template -notmatch '(?i)DeleteRegValue\s+HKCU\s+"Software\\Microsoft\\Windows\\CurrentVersion\\Run"'
    # The exact startup cleanup runs after the uninstall section's running-app check.
    scoped_startup_hook_retained = $template.IndexOf('!insertmacro NSIS_HOOK_POSTUNINSTALL') -gt $template.LastIndexOf('!insertmacro CheckIfAppIsRunning')
    downgrade_hook_retained = $template.Contains('!insertmacro NSIS_HOOK_PREINSTALL')
    app_payload_removal_retained = $template.Contains('Delete "$INSTDIR\${MAINBINARYNAME}.exe"')
    uninstaller_creation_retained = $template.Contains('WriteUninstaller "$INSTDIR\uninstall.exe"')
    upstream_license_retained = $license.Contains('Permission is hereby granted, free of charge')
}
if ($RenderedTemplate) {
    $rendered = Get-Content -LiteralPath (Resolve-Path -LiteralPath $RenderedTemplate).Path -Raw -Encoding UTF8
    $checks.rendered_no_appdata_delete_option = $rendered -notmatch 'DeleteAppDataCheckbox|\$\(deleteAppData\)'
    $checks.rendered_no_recursive_appdata_deletion = $rendered -notmatch '(?i)RmDir\s+/r\s+"\$(LOCALAPPDATA|APPDATA)\\'
    $checks.rendered_no_generic_startup_deletion = $rendered -notmatch '(?i)DeleteRegValue\s+HKCU\s+"Software\\Microsoft\\Windows\\CurrentVersion\\Run"'
    $checks.rendered_scoped_startup_hook = $rendered.IndexOf('!insertmacro NSIS_HOOK_POSTUNINSTALL') -gt $rendered.LastIndexOf('!insertmacro CheckIfAppIsRunning')
}
foreach ($name in $checks.Keys) { Write-Output "$name=$($checks[$name])" }
if ($checks.Values -contains $false) { throw 'Installer ownership checks failed.' }
Write-Output "$($checks.Count)/$($checks.Count) installer ownership checks passed."
