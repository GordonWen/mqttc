# Build the three portable Windows clients and package them.
#
#   .\build-windows.ps1              # release builds for egui + iced + gpui
#   .\build-windows.ps1 -SkipGpui    # faster: egui + iced only
#   .\build-windows.ps1 -Size        # extra release-size profile for size numbers
#
# Output: dist\mqttc-<ui>.exe plus dist\portable\mqttc-<ui>.zip
param(
    [switch]$SkipGpui,
    [switch]$Size
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$apps = @("mqttc-egui", "mqttc-iced")
if (-not $SkipGpui) { $apps += "mqttc-gpui" }

foreach ($app in $apps) {
    Write-Host "== release $app"
    if ($app -eq "mqttc-gpui") {
        # GPUI relies on unwinding internally, so it uses release-unwind.
        cargo build --profile release-unwind -p $app
        $profile = "release-unwind"
    } else {
        cargo build --release -p $app
        $profile = "release"
    }
    $exe = Join-Path $root "target\$profile\$app.exe"
    if (-not (Test-Path $exe)) { throw "missing $exe" }
    Copy-Item $exe (Join-Path $dist "$app.exe") -Force

    $stage = Join-Path $dist "portable\$app"
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item $exe (Join-Path $stage "mqttc.exe") -Force
    Copy-Item (Join-Path $root "tools\bench\fixtures\mqttc.json") (Join-Path $stage "mqttc.json") -Force
    "(portable) unzip anywhere and run mqttc.exe; config lives next to the exe." |
        Set-Content -Encoding UTF8 (Join-Path $stage "READ-ME.txt")
    $zip = Join-Path $dist "portable\$app.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -Force
    "{0}  {1:N1} MB" -f $app, ((Get-Item $exe).Length / 1MB) | Write-Host
}

if ($Size) {
    foreach ($app in @("mqttc-egui", "mqttc-iced")) {
        Write-Host "== release-size $app"
        cargo build --profile release-size -p $app
        $exe = Join-Path $root "target\release-size\$app.exe"
        Copy-Item $exe (Join-Path $dist "$app-size.exe") -Force
        "{0} (size profile)  {1:N1} MB" -f $app, ((Get-Item $exe).Length / 1MB) | Write-Host
    }
}

# Baseline: copy the untouched reference binary (never modify the source tree).
# The reference tree is kept locally and is not published, so skip when absent.
$refExe = Join-Path $root "参考实现\dist\mqttc.exe"
$baseline = Join-Path $dist "baseline-slint"
if (Test-Path $refExe) {
    New-Item -ItemType Directory -Force -Path $baseline | Out-Null
    Copy-Item $refExe (Join-Path $baseline "mqttc.exe") -Force
    Copy-Item (Join-Path $root "assets\NOTICE.md") (Join-Path $baseline "NOTICE.md") -Force -ErrorAction SilentlyContinue
} else {
    Write-Host "skip baseline: 参考实现\dist\mqttc.exe not present"
}

Get-ChildItem $dist -Filter "*.exe" | Sort-Object Length | Format-Table Name, @{n = "MB"; e = { [math]::Round($_.Length / 1MB, 1) } }
