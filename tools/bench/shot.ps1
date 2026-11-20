# Capture the client area of a window to a PNG, for locating click targets once.
#
#   .\shot.ps1 -Title "mqttc" -Out bench\out\baseline.png
#
# Coordinates in the saved image are client-relative physical pixels, the same
# space that `uia-click.ps1 -X -Y` expects.
param(
    [string]$Title = "mqttc",
    [string]$Out = "bench\out\baseline.png"
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Shot {
    [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr h, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr h, ref POINT pt);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    public struct RECT { public int Left, Top, Right, Bottom; }
    public struct POINT { public int X, Y; }
}
"@

[Shot]::SetProcessDPIAware() | Out-Null
$h = [Shot]::FindWindow([NullString]::Value, $Title)
if ($h -eq [IntPtr]::Zero) { throw "window '$Title' not found" }

$rect = New-Object Shot+RECT
[Shot]::GetClientRect($h, [ref]$rect) | Out-Null
$origin = New-Object Shot+POINT
[Shot]::ClientToScreen($h, [ref]$origin) | Out-Null

$w = $rect.Right - $rect.Left
$hgt = $rect.Bottom - $rect.Top
if ($w -le 0 -or $hgt -le 0) { throw "empty client rect" }

$dir = Split-Path -Parent $Out
if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }

$bmp = New-Object System.Drawing.Bitmap($w, $hgt)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bmp.Size)
$g.Dispose()
$bmp.Save((Resolve-Path $dir).Path + "\" + (Split-Path -Leaf $Out), [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()

"client {0}x{1} at screen {2},{3} -> {4}" -f $w, $hgt, $origin.X, $origin.Y, $Out
