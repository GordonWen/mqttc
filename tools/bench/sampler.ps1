# Sample working set / private bytes / CPU of a running process.
#
#   .\sampler.ps1 -ProcessId 1234 -Label "egui-s2" -Seconds 10 -Csv bench\out\samples.csv
#   .\sampler.ps1 -ProcessId 1234 -Label "egui-s1" -Timeline -Seconds 5
#
# CPU% is normalised by logical core count so it is comparable across machines.
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [string]$Label = "sample",
    [int]$Seconds = 10,
    [int]$IntervalMs = 1000,
    [string]$Csv = "",
    [switch]$Timeline
)

$ErrorActionPreference = "Stop"
$proc = Get-Process -Id $ProcessId
$cores = [Environment]::ProcessorCount
$startWall = Get-Date
$startCpu = $proc.TotalProcessorTime.TotalSeconds
$ws = New-Object System.Collections.Generic.List[double]
$priv = New-Object System.Collections.Generic.List[double]
$peak = 0.0
$handles = 0
$threads = 0

$deadline = $startWall.AddSeconds($Seconds)
while ((Get-Date) -lt $deadline) {
    try {
        $proc.Refresh()
        $ws.Add($proc.WorkingSet64 / 1MB)
        $priv.Add($proc.PrivateMemorySize64 / 1MB)
        if ($proc.PeakWorkingSet64 -gt $peak) { $peak = $proc.PeakWorkingSet64 / 1MB }
        $handles = $proc.HandleCount
        $threads = $proc.Threads.Count
        if ($Timeline) {
            "{0},{1},{2:N1},{3:N1}" -f (Get-Date -Format "HH:mm:ss.fff"), $Label, ($proc.WorkingSet64 / 1MB), ($proc.PrivateMemorySize64 / 1MB)
        }
    } catch { break }
    Start-Sleep -Milliseconds $IntervalMs
}

$endWall = Get-Date
try { $proc.Refresh(); $endCpu = $proc.TotalProcessorTime.TotalSeconds } catch { $endCpu = $startCpu }
$wall = ($endWall - $startWall).TotalSeconds
$cpu = if ($wall -gt 0) { (($endCpu - $startCpu) / $wall / $cores) * 100 } else { 0 }

function Median([System.Collections.Generic.List[double]]$values) {
    if ($values.Count -eq 0) { return 0 }
    $sorted = $values | Sort-Object
    # [int] rounds .5 to even, so use Floor for the 0-based middle index.
    $mid = [math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 0) { return ($sorted[$mid - 1] + $sorted[$mid]) / 2 }
    return $sorted[$mid]
}

$row = [pscustomobject]@{
    label       = $Label
    pid         = $ProcessId
    seconds     = [math]::Round($wall, 1)
    ws_min_mb   = if ($ws.Count) { [math]::Round(($ws | Measure-Object -Minimum).Minimum, 1) } else { 0 }
    ws_med_mb   = [math]::Round((Median $ws), 1)
    ws_max_mb   = if ($ws.Count) { [math]::Round(($ws | Measure-Object -Maximum).Maximum, 1) } else { 0 }
    priv_med_mb = [math]::Round((Median $priv), 1)
    peak_ws_mb  = [math]::Round($peak, 1)
    handles     = $handles
    threads     = $threads
    cpu_pct     = [math]::Round($cpu, 2)
}

if ($Csv) {
    $dir = Split-Path -Parent $Csv
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    if (Test-Path $Csv) {
        $row | Export-Csv -Path $Csv -NoTypeInformation -Append
    } else {
        $row | Export-Csv -Path $Csv -NoTypeInformation
    }
}
$row | Format-List
