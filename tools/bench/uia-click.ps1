# Drive the un-instrumented Slint baseline through its window.
#
# The reference binary accepts no CLI parameters, so the benchmark pins its
# window to a known position, then clicks a coordinate relative to the window
# client area. Verify the offsets once per resolution / DPI and pass them in.
#
#   .\uia-click.ps1 -Title "mqttc" -X 763 -Y 46           # click "Connect"
#   .\uia-click.ps1 -Title "mqttc" -X 763 -Y 46 -Move 0,0 -Size 1360,860
param(
    [string]$Title = "mqttc",
    [int]$X = 0,
    [int]$Y = 0,
    [string]$Move = "",
    [string]$Size = ""
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr h, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr h, ref POINT pt);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    public struct RECT { public int Left, Top, Right, Bottom; }
    public struct POINT { public int X, Y; }
}
"@

# Match physical-pixel coordinates on a scaled display; without this Windows
# virtualises ClientToScreen and the click lands in the wrong place.
[Win]::SetProcessDPIAware() | Out-Null

# PowerShell marshals $null as "" for a [string] parameter, which FindWindow
# treats as a class-name filter; pass a real NULL instead.
$h = [Win]::FindWindow([NullString]::Value, $Title)
if ($h -eq [IntPtr]::Zero) { throw "window '$Title' not found" }

if ($Move -and $Size) {
    $m = $Move.Split(","); $s = $Size.Split(",")
    [Win]::SetWindowPos($h, [IntPtr]::Zero, [int]$m[0], [int]$m[1], [int]$s[0], [int]$s[1], 0x0040) | Out-Null
    Start-Sleep -Milliseconds 300
}

[Win]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 200
$pt = New-Object Win+POINT
$pt.X = $X; $pt.Y = $Y
[Win]::ClientToScreen($h, [ref]$pt) | Out-Null
[Win]::SetCursorPos($pt.X, $pt.Y) | Out-Null
Start-Sleep -Milliseconds 100
[Win]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # left down
[Win]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # left up
Write-Host "clicked ($X,$Y) in '$Title'"
