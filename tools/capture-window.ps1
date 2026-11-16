# Captures a window of the desktop app, optionally scrolls it first.
# Usage: powershell -ExecutionPolicy Bypass -File tools\capture-window.ps1 [-Process recast-app] [-Scroll -5] [-Out path.png]
param(
    [string] $Process = "recast-app",
    [int] $Scroll = 0,
    [string] $Out = "$env:TEMP\opencode\window.png",
    [switch] $Panel
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint dx, uint dy, int data, UIntPtr extra);
    public const uint WHEEL = 0x0800;
}
"@

$proc = Get-Process -Name $Process -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Write-Error "Process '$Process' not found"; exit 1 }
$hwnd = $proc.MainWindowHandle

$rect = New-Object Win+RECT
[Win]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top

if ($Scroll -ne 0) {
    $x = if ($Panel) { $rect.Right - 140 } else { $rect.Left + $width / 2 }
    $y = if ($Panel) { $rect.Top + $height / 2 } else { $rect.Top + $height / 2 }
    [Win]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 250
    $steps = [math]::Abs($Scroll)
    for ($i = 0; $i -lt $steps; $i++) {
        $delta = if ($Scroll -lt 0) { -120 } else { 120 }
        [Win]::mouse_event([Win]::WHEEL, 0, 0, $delta, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 120
    }
    Start-Sleep -Milliseconds 600
}

$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$hdc = $graphics.GetHdc()
[Win]::PrintWindow($hwnd, $hdc, 2) | Out-Null
$graphics.ReleaseHdc($hdc)

if ($Panel) {
    $crop = New-Object System.Drawing.Rectangle(($width - 400), 40, 395, ($height - 80))
    $part = $bitmap.Clone($crop, $bitmap.PixelFormat)
    $part.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $part.Dispose()
} else {
    $bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
}
$graphics.Dispose()
$bitmap.Dispose()
Write-Host "Saved $Out"
