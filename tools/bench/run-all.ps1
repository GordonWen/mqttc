# Reproduce the benchmark matrix on Windows.
#
#   .\run-all.ps1                     # all apps, scenarios s1..s4, 3 runs each
#   .\run-all.ps1 -Apps egui -Runs 1  # quick smoke
#
# Prerequisites:
#   * cargo build --release -p mqttc-egui -p mqttc-iced -p mqttc-gpui
#   * cargo build --release -p mqttc-test-broker
#   * Windows PowerShell 5.1+
#
# The script starts three local brokers so every app sees identical data:
#   1883: 200 retained messages (S2)
#   1884: idle (S1 / S4)
#   1885: stream @ 10 msg/s (S3)
# It writes one CSV per run under bench\out and prints a median summary.
param(
    [string[]]$Apps = @("egui", "iced", "gpui"),
    [string[]]$Scenarios = @("s1", "s2", "s3", "s4"),
    [int]$Runs = 3,
    [string]$OutDir = "bench\out",
    [string]$Config = "tools\bench\fixtures\mqttc.json",
    [int]$RetainedPort = 1883,
    [int]$IdlePort = 1884,
    [int]$StreamPort = 1885
)

$ErrorActionPreference = "Stop"
if ($Apps.Count -eq 1 -and $Apps[0] -like "*,*") { $Apps = $Apps[0].Split(",") | ForEach-Object { $_.Trim() } }
if ($Scenarios.Count -eq 1 -and $Scenarios[0] -like "*,*") { $Scenarios = $Scenarios[0].Split(",") | ForEach-Object { $_.Trim() } }
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$brokerExe = Join-Path $root "target\release\mqttc-test-broker.exe"
if (-not (Test-Path $brokerExe)) { throw "missing $brokerExe (cargo build --release -p mqttc-test-broker)" }

$brokers = @()
function Test-Port([int]$Port) {
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $task = $client.ConnectAsync("127.0.0.1", $Port)
        return $task.Wait(300) -and $client.Connected
    } catch { return $false } finally { $client.Dispose() }
}
function Start-Broker([int]$Port, [int]$Prepare, [int]$Stream) {
    if (Test-Port $Port) { Write-Host "reuse broker on $Port"; return $null }
    $args = @("--port", "$Port")
    if ($Prepare -gt 0) { $args += @("--prepare", "$Prepare") }
    if ($Stream -gt 0) { $args += @("--stream", "$Stream") }
    $p = Start-Process -FilePath $brokerExe -ArgumentList $args -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 2
    return $p
}

# One broker per scenario keeps retained fixtures and streams isolated.
$brokers += Start-Broker $RetainedPort 200 0
$brokers += Start-Broker $IdlePort 0 0
$brokers += Start-Broker $StreamPort 0 10

function Fixture([int]$Port) {
    $cfg = Get-Content $Config -Raw | ConvertFrom-Json
    $cfg.connections[0].port = "$Port"
    $path = Join-Path $OutDir "fixture-$Port.json"
    $cfg | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 $path
    return $path
}

$scenarioPort = @{ s1 = $IdlePort; s2 = $RetainedPort; s3 = $StreamPort; s4 = $IdlePort }
$scenarioQuit = @{ s1 = 40; s2 = 75; s3 = 70; s4 = 70 }
$scenarioSample = @{ s1 = 5; s2 = 60; s3 = 60; s4 = 60 }

$summary = @()
try {
    foreach ($app in $Apps) {
        $exe = Join-Path $root "target\release\mqttc-$app.exe"
        if (-not (Test-Path $exe)) {
            # GPUI is delivered from the unwind-preserving profile.
            $exe = Join-Path $root "target\release-unwind\mqttc-$app.exe"
        }
        if (-not (Test-Path $exe)) { Write-Warning "skip $app (missing $exe)"; continue }
        foreach ($scenario in $Scenarios) {
            $port = $scenarioPort[$scenario]
            $fixture = Fixture $port
            for ($run = 1; $run -le $Runs; $run++) {
                $label = "$app-$scenario-run$run"
                $csv = Join-Path $OutDir "$label.csv"
                $p = Start-Process -FilePath $exe -PassThru -WindowStyle Minimized -ArgumentList @(
                    "--bench", "--bench-scenario", $scenario,
                    "--bench-quit-after", "$($scenarioQuit[$scenario])",
                    "--config", $fixture
                )
                # Warm up (first frame / connect / retained burst), then sample.
                $sample = Join-Path $OutDir "summary.csv"
                $sampler = Join-Path $PSScriptRoot "sampler.ps1"
                if ($scenario -eq "s1") {
                    # Startup snapshots at 2 s, 5 s and a 30 s plateau.
                    Start-Sleep -Seconds 2
                    & $sampler -ProcessId $p.Id -Label "$label-2s" -Seconds 1 -IntervalMs 250 -Csv $sample | Out-Null
                    Start-Sleep -Seconds 2
                    & $sampler -ProcessId $p.Id -Label "$label-5s" -Seconds 1 -IntervalMs 250 -Csv $sample | Out-Null
                    Start-Sleep -Seconds 24
                    & $sampler -ProcessId $p.Id -Label "$label-30s" -Seconds 5 -IntervalMs 1000 -Csv $sample | Out-Null
                } else {
                    Start-Sleep -Seconds 5
                    & $sampler -ProcessId $p.Id -Label $label -Seconds $scenarioSample[$scenario] `
                        -IntervalMs 1000 -Csv $sample | Out-Null
                }
                if (-not $p.HasExited) { $p.WaitForExit(120000) | Out-Null }
                Write-Host "done $label"
            }
        }
    }
} finally {
    foreach ($b in $brokers) { if ($b -and -not $b.HasExited) { Stop-Process -Id $b.Id -Force -ErrorAction SilentlyContinue } }
}

$summaryPath = Join-Path $OutDir "summary.csv"
if (Test-Path $summaryPath) {
    $rows = Import-Csv $summaryPath
    $rows | Group-Object { ($_.label -split '-run')[0] } | ForEach-Object {
        $ws = $_.Group.ws_med_mb | ForEach-Object { [double]$_ } | Sort-Object
        $cpu = $_.Group.cpu_pct | ForEach-Object { [double]$_ } | Sort-Object
        $n = $ws.Count
        # [int] rounds .5 to even; Floor picks the lower middle for odd counts.
        [pscustomobject]@{
            case    = $_.Name
            runs    = $n
            ws_med  = if ($n) { $ws[[math]::Floor($n / 2)] } else { 0 }
            ws_max  = if ($n) { $ws[$n - 1] } else { 0 }
            cpu_med = if ($n) { $cpu[[math]::Floor($n / 2)] } else { 0 }
        }
    } | Sort-Object case | Format-Table -AutoSize
}
