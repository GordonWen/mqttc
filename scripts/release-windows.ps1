# Build the Windows release asset for mqttc (iced) and write a SHA256 file.
#
#   powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1 -Version 0.2.0
#
# Output (not committed, ignored by .gitignore):
#   dist\release\mqttc-v<version>-windows-x64.zip
#   dist\release\mqttc-v<version>-windows-x64.zip.sha256
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

if (-not $Version) {
    $line = Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"$' |
        Select-Object -First 1
    if (-not $line) { throw "cannot read workspace version from Cargo.toml" }
    $Version = $line.Matches[0].Groups[1].Value
}

Write-Host "== build mqttc-iced $Version (release)"
cargo build --release -p mqttc-iced
$exe = Join-Path $root "target\release\mqttc-iced.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }

$out = Join-Path $root "dist\release"
$stage = Join-Path $out "stage-windows-x64"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $exe (Join-Path $stage "mqttc.exe") -Force
Copy-Item (Join-Path $root "tools\bench\fixtures\mqttc.json") (Join-Path $stage "mqttc.json") -Force
$readme = @"
mqttc $Version - portable Windows x64 build
===========================================

1. Unzip this folder anywhere (a path with spaces or Chinese characters is fine).
2. Run mqttc.exe.
3. Configuration and exported logs are written next to mqttc.exe:
   mqttc.json and logs\.
4. No installer, no registry entries, no files in your user profile.

Requirements: Windows 10 1809+ or Windows 11. The Visual C++ runtime
(VCRUNTIME140) is normally already present; if not, install the Microsoft
Visual C++ 2015-2022 Redistributable (x64).

Source, issues and other platforms: https://github.com/GordonWen/mqttc
License: MIT (see LICENSE in the repository).
"@
[IO.File]::WriteAllText((Join-Path $stage "READ-ME.txt"), $readme, (New-Object Text.UTF8Encoding($false)))

$zipName = "mqttc-v$Version-windows-x64.zip"
$zip = Join-Path $out $zipName
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -Force

$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
$sum = Join-Path $out ($zipName + ".sha256")
[IO.File]::WriteAllText($sum, "$hash  $zipName`n", (New-Object Text.UTF8Encoding($false)))

Write-Host ""
Write-Host ("asset : {0}" -f $zip)
Write-Host ("sha256: {0}" -f $sum)
Write-Host ("size  : {0:N1} MB" -f ((Get-Item $zip).Length / 1MB))
