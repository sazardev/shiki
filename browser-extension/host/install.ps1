# Installs shiki-native-host on Windows and registers the native messaging manifest
param(
  [string]$ExtensionId = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
$HostBin = Join-Path $RepoRoot "target\release\shiki-native-host.exe"
$Template = Join-Path $PSScriptRoot "com.shiki.native.json"

Write-Host "==> Building shiki-native-host (release)..."
cargo build --release -p shiki-native-host
if (-not (Test-Path $HostBin)) { throw "Build failed: $HostBin not found" }
Write-Host "    built: $HostBin"

if (-not $ExtensionId) {
  Write-Host "    note: no -ExtensionId given, using placeholder. Re-run with your extension ID after loading unpacked."
  $ExtensionId = "__REPLACE_WITH_EXTENSION_ID__"
}

# Chrome on Windows uses registry, not file. We write manifest via JSON to handle escaping.
$ManifestDir = "$env:APPDATA\shiki"
New-Item -ItemType Directory -Force -Path $ManifestDir | Out-Null
$Dest = Join-Path $ManifestDir "com.shiki.native.json"
$manifest = Get-Content $Template -Raw | ConvertFrom-Json
$manifest.path = $HostBin
$manifest.allowed_origins = @("chrome-extension://$ExtensionId/")
$manifest | ConvertTo-Json -Depth 3 | Set-Content -Path $Dest -Encoding UTF8
Write-Host "    installed manifest: $Dest"
Get-Content $Dest | Write-Host

# Register for Chrome
$RegPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.shiki.native"
New-Item -Path $RegPath -Force | Out-Null
Set-ItemProperty -Path $RegPath -Name "(Default)" -Value $Dest
Write-Host "    registered: $RegPath -> $Dest"

# Also register for Edge/Brave if present
foreach ($browser in @("Microsoft\Edge","BraveSoftware\Brave-Browser")) {
  $p = "HKCU:\Software\$browser\NativeMessagingHosts\com.shiki.native"
  try { New-Item -Path $p -Force | Out-Null; Set-ItemProperty -Path $p -Name "(Default)" -Value $Dest; Write-Host "    registered: $p" } catch {}
}

Write-Host ""
Write-Host "Done. Next: chrome://extensions -> Developer mode -> Load unpacked -> select browser-extension\"
Write-Host "Then re-run: .\install.ps1 -ExtensionId <id>"
