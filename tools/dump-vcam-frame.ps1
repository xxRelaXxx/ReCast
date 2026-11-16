# Dumps the frame that ReCast delivers to the virtual camera.
# Usage: powershell -ExecutionPolicy Bypass -File tools\dump-vcam-frame.ps1 [out.png]
param([string] $Out = "$env:TEMP\vcam_frame.png")

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Native {
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, SetLastError = true)]
    public static extern IntPtr OpenFileMapping(uint access, bool inherit, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr MapViewOfFile(IntPtr handle, uint access, uint offset, uint size, uint alloc);
    [DllImport("kernel32.dll")]
    public static extern bool UnmapViewOfFile(IntPtr ptr);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, SetLastError = true)]
    public static extern IntPtr OpenMutex(uint access, bool inherit, string name);
    [DllImport("kernel32.dll")]
    public static extern bool ReleaseMutex(IntPtr handle);
}
"@

$name = if ($env:VCAM_SHM) { $env:VCAM_SHM } else { "ReCast Virtual Camera/SharedMemory" }
$handle = [Native]::OpenFileMapping(0x0004, $false, $name)   # FILE_MAP_READ
if ($handle -eq [IntPtr]::Zero) {
    Write-Error "Cannot open the shared memory '$name' (error $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
    exit 1
}
$ptr = [Native]::MapViewOfFile($handle, 0x0004, 0, 0, 0)
if ($ptr -eq [IntPtr]::Zero) {
    Write-Error "Cannot map the shared memory (error $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
    exit 1
}

function Read-Bytes([IntPtr] $address, [int] $offset, [int] $count) {
    $out = New-Object byte[] $count
    [System.Runtime.InteropServices.Marshal]::Copy([IntPtr]($address.ToInt64() + $offset), $out, 0, $count)
    return $out
}

$header = Read-Bytes $ptr 0 24
$imageOffset = [BitConverter]::ToUInt32($header, 0)
$width = [BitConverter]::ToUInt16($header, 4)
$height = [BitConverter]::ToUInt16($header, 6)
$active = $header[12]
Write-Host "header: offset=$imageOffset width=$width height=$height active=$active"

$size = $width * $height * 3
$bytes = Read-Bytes $ptr $imageOffset $size
[Native]::UnmapViewOfFile($ptr) | Out-Null

$bmp = New-Object System.Drawing.Bitmap($width, $height)
$rect = New-Object System.Drawing.Rectangle(0, 0, $width, $height)
$data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$row = New-Object byte[] ($width * 3)
for ($y = 0; $y -lt $height; $y++) {
    [Array]::Copy($bytes, $y * $width * 3, $row, 0, $width * 3)
    for ($x = 0; $x -lt $width; $x++) {
        $b = $row[$x * 3]
        $g = $row[$x * 3 + 1]
        $r = $row[$x * 3 + 2]
        $row[$x * 3] = $r
        $row[$x * 3 + 1] = $g
        $row[$x * 3 + 2] = $b
    }
    [System.Runtime.InteropServices.Marshal]::Copy($row, 0, [IntPtr]([Int64]$data.Scan0 + [Int64]$y * $data.Stride), $width * 3)
}
$bmp.UnlockBits($data)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Saved $Out ($width x $height)"
