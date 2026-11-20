# Smoke-test every app against the two external brokers in servers.json.
#
# Reads credentials from tools/bench/servers.json (local only, gitignored) and
# writes throw-away configs to bench/out/external/. Nothing secret is echoed.
#
#   .\external.ps1                    # all apps, both servers, subscribe + timed publish
#   .\external.ps1 -Apps egui -QuitS4 20
param(
    [string]$Servers = "tools\bench\servers.json",
    [string]$OutDir = "bench\out\external",
    [string[]]$Apps = @("egui", "iced", "gpui"),
    [int]$QuitS2 = 20,
    [int]$QuitS4 = 25,
    [string]$Protocol = "",
    [string]$Only = ""
)

$ErrorActionPreference = "Stop"
# `-File script.ps1 -Apps iced,gpui` passes one string; split like run-all.ps1.
if ($Apps.Count -eq 1 -and $Apps[0] -like "*,*") { $Apps = $Apps[0].Split(",") | ForEach-Object { $_.Trim() } }
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
if (-not (Test-Path $Servers)) { throw "missing $Servers (local credentials file)" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$serversFile = Get-Content $Servers -Raw | ConvertFrom-Json
$exeMap = @{
    egui = "target\release\mqttc-egui.exe"
    iced = "target\release\mqttc-iced.exe"
    gpui = "target\release-unwind\mqttc-gpui.exe"
}

function Write-Config([string]$path, $srv, [string]$topic, [string]$proto) {
    $cfg = [ordered]@{
        active_id = "ext"
        log_format = "text"
        auto_scroll = $true
        show_advanced = $false
        log_filter = ""
        log_kind_filter = 0
        skin = "black"
        language = "en"
        connections = @(
            [ordered]@{
                id = "ext"
                name = "external"
                host = $srv.host
                port = "$($srv.port)"
                client_id = "mqttc-ext-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
                username = "$($srv.username)"
                password = "$($srv.password)"
                sub_topic = $topic
                sub_qos = 0
                pub_topic = $topic
                pub_payload = "hello from mqttc"
                pub_qos = 0
                pub_retain = $false
                pub_format = "text"
                keep_alive = 60
                clean_session = $true
                connect_timeout = 10
                will_topic = ""
                will_payload = ""
                will_qos = 0
                will_retain = $false
                ssl = [bool]$srv.tls
                ssl_verify = if ($null -ne $srv.verify) { [bool]$srv.verify } else { $true }
                ssl_ca = "$($srv.ca)"
                ssl_cert = "$($srv.cert)"
                ssl_key = "$($srv.key)"
                protocol = $proto
                timed_interval = 5
                subscriptions = @(@{ topic = $topic; qos = 0 })
                publish_presets = @()
                auto_reply_rules = @()
                auto_reply_echo = "skip"
            }
        )
    }
    $json = $cfg | ConvertTo-Json -Depth 12
    [IO.File]::WriteAllText($path, $json, (New-Object System.Text.UTF8Encoding($false)))
}

$stamp = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$results = @()
foreach ($srv in $serversFile.servers) {
    $alias = $srv.name
    if ($Only -and $alias -ne $Only) { continue }
    $proto = if ($Protocol) { $Protocol } else { if ($srv.protocol) { "$($srv.protocol)" } else { "3.1.1" } }
    foreach ($app in $Apps) {
        $exe = $exeMap[$app]
        if (-not (Test-Path $exe)) { Write-Warning "skip $app (missing $exe)"; continue }
        foreach ($scenario in @("s2", "s4")) {
            $quit = if ($scenario -eq "s2") { $QuitS2 } else { $QuitS4 }
            $slug = "$alias-$app-$scenario"
            $topic = "$($srv.test_topic)/$stamp-$app-$scenario"
            $cfgPath = Join-Path $OutDir "$slug.json"
            Write-Config $cfgPath $srv $topic $proto
            $stdout = Join-Path $OutDir "$slug.out.txt"
            $stderr = Join-Path $OutDir "$slug.err.txt"
            $p = Start-Process -FilePath $exe -PassThru -WindowStyle Minimized `
                -ArgumentList @("--bench", "--bench-scenario", $scenario,
                    "--bench-quit-after", "$quit", "--config", $cfgPath) `
                -RedirectStandardOutput $stdout -RedirectStandardError $stderr
            if (-not $p.WaitForExit(($quit + 45) * 1000)) {
                Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
                Write-Warning "$slug timed out"
            }
            $out = if (Test-Path $stdout) { Get-Content $stdout -Raw } else { "" }
            $m = [regex]::Match($out, "BENCH_SUMMARY logs=(\d+) recv=(\d+) send=(\d+) status=(\w+)")
            $results += [pscustomobject]@{
                server   = $alias
                app      = $app
                scenario = $scenario
                status   = if ($m.Success) { $m.Groups[4].Value } else { "no-summary" }
                recv     = if ($m.Success) { [int]$m.Groups[2].Value } else { -1 }
                send     = if ($m.Success) { [int]$m.Groups[3].Value } else { -1 }
            }
            Write-Host ("{0,-16} {1,-5} {2}  status={3} recv={4} send={5}" -f `
                $alias, $app, $scenario, $results[-1].status, $results[-1].recv, $results[-1].send)
        }
    }
}

$csv = Join-Path $OutDir $(if ($Protocol) { "external-summary-$Protocol.csv" } else { "external-summary.csv" })
# Merge with earlier runs so invoking the script per app keeps every row.
$merged = @($results)
if (Test-Path $csv) {
    $seen = @{}
    foreach ($r in $results) { $seen["$($r.server)|$($r.app)|$($r.scenario)"] = $true }
    $old = Import-Csv $csv | Where-Object { -not $seen["$($_.server)|$($_.app)|$($_.scenario)"] }
    $merged = @($old) + @($results)
}
$merged | Sort-Object server, app, scenario | Export-Csv -Path $csv -NoTypeInformation -Encoding UTF8
Write-Host "wrote $csv"
