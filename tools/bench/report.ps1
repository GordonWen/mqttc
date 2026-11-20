# Turn bench\out\summary.csv into markdown tables for docs\BENCHMARK.md.
#
#   .\report.ps1                        # prints markdown to stdout
#   .\report.ps1 -Csv other.csv -Out bench\out\tables.md
#
# Use -Out on a Chinese console: redirected stdout is GBK and mangles the
# headings, while -Out writes UTF-8.
param(
    [string]$Csv = "bench\out\summary.csv",
    [string]$Out = ""
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root

if (-not (Test-Path $Csv)) { throw "missing $Csv (run run-all.ps1 first)" }
$rows = Import-Csv $Csv

$lines = New-Object System.Collections.Generic.List[string]
function Emit([string]$text) { $lines.Add($text); $text }

function Med($values) {
    $sorted = @($values | ForEach-Object { [double]$_ } | Sort-Object)
    if ($sorted.Count -eq 0) { return 0 }
    # [int] rounds .5 to even, so use Floor for the 0-based middle index.
    $mid = [math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 0) { return ($sorted[$mid - 1] + $sorted[$mid]) / 2 }
    return $sorted[$mid]
}

Emit "| 场景 | 工作集 中位 (MB) | 工作集 最大 (MB) | 峰值 (MB) | 私有 中位 (MB) | CPU% (按核归一) | 线程 | 句柄 |"
Emit "|---|---|---|---|---|---|---|---|"

$rows |
    Group-Object { $_.label -replace "-run\d+", "" } |
    Sort-Object Name |
    ForEach-Object {
        $g = $_.Group
        $ws = Med ($g.ws_med_mb)
        $wsMax = ($g.ws_max_mb | ForEach-Object { [double]$_ } | Measure-Object -Maximum).Maximum
        $peak = ($g.peak_ws_mb | ForEach-Object { [double]$_ } | Measure-Object -Maximum).Maximum
        $priv = Med ($g.priv_med_mb)
        $cpu = Med ($g.cpu_pct)
        $thr = Med ($g.threads)
        $han = Med ($g.handles)
        Emit ("| {0} | {1:N1} | {2:N1} | {3:N1} | {4:N1} | {5:N2} | {6:N0} | {7:N0} |" -f `
            $_.Name, $ws, $wsMax, $peak, $priv, $cpu, $thr, $han) | Out-Null
    }

Emit "" | Out-Null
Emit "| 可执行文件 | 大小 (MB) |" | Out-Null
Emit "|---|---|" | Out-Null
$exes = @(
    "target\release\mqttc-egui.exe",
    "target\release\mqttc-iced.exe",
    "target\release-unwind\mqttc-gpui.exe",
    "target\release-size\mqttc-egui.exe",
    "target\release-size\mqttc-iced.exe",
    "dist\baseline-slint\mqttc.exe"
)
foreach ($exe in $exes) {
    if (Test-Path $exe) {
        $name = (Split-Path -Leaf (Split-Path -Parent $exe)) + "\" + (Split-Path -Leaf $exe)
        Emit ("| {0} | {1:N2} |" -f $name, ((Get-Item $exe).Length / 1MB)) | Out-Null
    }
}
$zips = Get-ChildItem "dist\portable\*.zip" -ErrorAction SilentlyContinue
foreach ($zip in $zips) {
    Emit ("| portable: {0} | {1:N2} |" -f $zip.Name, ($zip.Length / 1MB)) | Out-Null
}

if ($Out) {
    $full = Join-Path $root $Out
    $dir = Split-Path -Parent $full
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    [IO.File]::WriteAllLines($full, $lines, (New-Object System.Text.UTF8Encoding($false)))
    "wrote $Out"
}
