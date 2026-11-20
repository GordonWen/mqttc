# Measure the reference Slint binary, which accepts no CLI flags.
#
# The un-instrumented baseline is driven through its window: the script pins
# the window to a fixed client area, clicks "Connect" (and, for S4, "Timed"),
# then samples with the same sampler.ps1 used for the new apps.
#
#   .\baseline.ps1 -Scenario s1 -Runs 3
#   .\baseline.ps1 -Scenario s2 -Runs 3 -Port 1883
#   .\baseline.ps1 -Scenario s3 -Runs 3 -Port 1893
#   .\baseline.ps1 -Scenario s4 -Runs 3 -Port 1892 -TimedX 1228 -TimedY 756
#
# -ClickX/-ClickY and -TimedX/-TimedY are **client-relative physical pixels**;
# re-verify them with shot.ps1 after changing the window size or display DPI.
param(
    [string]$Exe = "dist\baseline-slint\mqttc.exe",
    [ValidateSet("s1", "s2", "s3", "s4")][string]$Scenario = "s2",
    [int]$Runs = 3,
    [int]$Port = 1883,
    [int]$ClickX = 1522,
    [int]$ClickY = 47,
    [int]$TimedX = -1,
    [int]$TimedY = -1,
    [string]$Title = "mqttc",
    [string]$Config = "tools\bench\fixtures\mqttc.json",
    [string]$OutDir = "bench\out",
    [string]$WinSize = "1712,1109"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

if (-not (Test-Path $Exe)) { throw "missing $Exe" }
$exeFull = (Resolve-Path $Exe).Path
$exeDir = Split-Path -Parent $exeFull

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class BaseWin {
    [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
}
"@
[BaseWin]::SetProcessDPIAware() | Out-Null

# One config per run: the baseline always reads mqttc.json next to the exe.
function Write-Fixture([int]$p) {
    $cfg = Get-Content $Config -Raw | ConvertFrom-Json
    $cfg.connections[0].port = "$p"
    $json = $cfg | ConvertTo-Json -Depth 12
    $path = Join-Path $exeDir "mqttc.json"
    [IO.File]::WriteAllText($path, $json, (New-Object System.Text.UTF8Encoding($false)))
}

function Wait-Window([int]$timeoutMs = 15000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) {
        $h = [BaseWin]::FindWindow([NullString]::Value, $Title)
        if ($h -ne [IntPtr]::Zero) { return $h }
        Start-Sleep -Milliseconds 200
    }
    throw "window '$Title' not found"
}

function Pin-Window([IntPtr]$h) {
    $s = $WinSize.Split(",")
    [BaseWin]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, [int]$s[0], [int]$s[1], 0x0040) | Out-Null
    [BaseWin]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 400
}

function Click-At([int]$x, [int]$y) {
    & (Join-Path $PSScriptRoot "uia-click.ps1") -Title $Title -X $x -Y $y | Out-Null
    Start-Sleep -Milliseconds 500
}

$sampler = Join-Path $PSScriptRoot "sampler.ps1"
$summary = Join-Path $OutDir "summary.csv"

for ($run = 1; $run -le $Runs; $run++) {
    $label = "slint-$Scenario-run$run"
    Write-Fixture $Port
    $p = Start-Process -FilePath $exeFull -WorkingDirectory $exeDir -PassThru
    try {
        Start-Sleep -Seconds 2
        $h = Wait-Window
        Pin-Window $h

        if ($Scenario -eq "s1") {
            Start-Sleep -Seconds 1
            & $sampler -ProcessId $p.Id -Label "$label-2s" -Seconds 1 -IntervalMs 250 -Csv $summary | Out-Null
            Start-Sleep -Seconds 2
            & $sampler -ProcessId $p.Id -Label "$label-5s" -Seconds 1 -IntervalMs 250 -Csv $summary | Out-Null
            Start-Sleep -Seconds 24
            & $sampler -ProcessId $p.Id -Label "$label-30s" -Seconds 5 -IntervalMs 1000 -Csv $summary | Out-Null
        } else {
            Click-At $ClickX $ClickY
            if ($Scenario -eq "s4") {
                if ($TimedX -lt 0 -or $TimedY -lt 0) { throw "s4 needs -TimedX/-TimedY (client pixels)" }
                Start-Sleep -Seconds 2
                Click-At $TimedX $TimedY
            }
            Start-Sleep -Seconds 5
            & $sampler -ProcessId $p.Id -Label $label -Seconds 60 -IntervalMs 1000 -Csv $summary | Out-Null
        }
    } finally {
        if (-not $p.HasExited) {
            $p.CloseMainWindow() | Out-Null
            if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
        }
        Start-Sleep -Seconds 1
    }
    Write-Host "done $label"
}
