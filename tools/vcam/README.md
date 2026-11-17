# The virtual camera filter

ReCast needs a camera that other applications can open. On Windows that means a
DirectShow source filter registered under the *Video Input Device* category, so that
Zoom, Discord, OBS, Chrome, Edge and every other application finds it in its camera
list.

This directory contains that filter.

| | |
| --- | --- |
| Upstream | [tshino/softcam](https://github.com/tshino/softcam) v1.8.1 |
| Licence | MIT, Copyright (c) 2020 tshino |
| Filter name | `ReCast Camera` |
| CLSID | `{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}` |
| Legacy alias | `{AEF3B972-5FA5-4647-9571-358EB472BC9E}` |

## Files

```text
x64/softcam.dll        64-bit filter, for 64-bit applications
x86/softcam.dll        32-bit filter, for 32-bit applications
source/                the complete fork of softcam
recast-softcam.patch the full diff against upstream v1.8.1
```

Checksums of the committed binaries:

```text
fe9407d07e86e492077902e7cbbbc6d3c557ffdd8f90d86e053b366dfcaaf753  x64/softcam.dll
4e819a80d6b96dc352989f6bfb2b07c582527c7a669c34f3cba2ce2912e664fa  x86/softcam.dll
```

## Why a fork

The upstream project is MIT licensed and does exactly what is needed, but ReCast needs
a few changes:

* **Its own CLSID.** The shared-memory segment that carries the frames is named after the
  filter. Two different softcam builds would otherwise fight over the same segment, and
  an old installation could keep serving frames to the new one.
* **A new name.** The filter shows up as *ReCast Camera* instead of
  *DirectShow Softcam*.
* **A legacy alias.** The old CLSID is registered as well, so applications that already
  selected the camera keep working until the old registration is removed.
* **A version byte** in the frame-buffer protocol, so an old and a new build cannot
  misinterpret each other's frames.
* **The formats browsers and Discord actually connect.** The first type is I420.
  The filter also offers NV12, YUY2, UYVY, RGB24 and RGB32 at the usual sizes
  (320x240, 640x360, 640x480, 960x540, 1280x720 and 1920x1080, at 15, 30 and 60 fps
  where listed). It scales the sender's picture into the size the application picked,
  and it still answers when the phone is not connected yet. A camera that offered
  only one exact RGB24 mode was reported by sites as "already in use" or "not
  supported", and Discord's call showed a black picture.

The rebrand against upstream is in `recast-softcam.patch`. The browser-format work lives
in the source and has not been folded back into that patch.

## Building

The filter is plain C++ and needs Visual Studio 2022 (or the Build Tools with the
*Desktop development with C++* workload).

```powershell
msbuild tools\vcam\source\softcam.sln /p:Configuration=Release /p:Platform=x64
msbuild tools\vcam\source\softcam.sln /p:Configuration=Release /p:Platform=Win32
```

The results are `source\x64\Release\softcam.dll` and `source\Win32\Release\softcam.dll`.
Copy them to `x64\softcam.dll` and `x86\softcam.dll`.

For Visual Studio 2019, open `source\softcam_vs2019.sln` instead.

## How it is registered

The launcher writes the registry entries for the current user, in the 64-bit and the
32-bit registry view:

```text
HKCU\Software\Classes\CLSID\{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}\InprocServer32
HKCU\Software\Classes\CLSID\{AEF3B972-5FA5-4647-9571-358EB472BC9E}\InprocServer32
HKCU\Software\Classes\CLSID\{860BB310-5D01-11D0-BD3B-00A0C911CE86}\Instance\ReCast Camera
```

The `Instance` key carries the `REGFILTER2` blob that describes the single video output
pin, and the `InprocServer32` key points at the DLL in
`%LOCALAPPDATA%\ReCast\vcam`.

Registering per user means that no administrator rights are needed and that the camera
disappears when the user profile is removed.

## Sending frames

The desktop app loads the DLL at run time with `libloading` and uses the upstream sender
API:

```c
scCamera camera = scCreateCamera(1920, 1080, 0.0f);
scSendFrame(camera, rgb_bytes);   // 24-bit RGB, one frame
```

The frames are handed over through a named shared-memory segment and a mutex, so no data
is copied across the process boundary more than once per frame.
