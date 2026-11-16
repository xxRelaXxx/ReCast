# Changelog

All notable changes to ReCast are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-11-16

A new interface, separate camera and microphone modes, a USB connection that needs no
network at all and an installer with a setup window.

### Added

- **Installer window**: `desktop/ReCast.exe` now opens a setup window in the same style as
  the app. It asks for the folder, offers a desktop shortcut, a start menu entry and a
  start with Windows, shows a progress bar that fills up while the runtime is copied, and
  finally offers to start ReCast and to remove the installer and its own files.
- **Connection over the USB cable alone**: the new **Connect over USB** button opens a
  tunnel through the cable (`adb reverse`) and starts the phone app with the address and
  PIN filled in. The desktop shows whether the phone is connected over the cable or over
  Wi-Fi.
- **Microphone gain** from 0 to 300 % in the *Audio output* section. The gain is applied
  before the signal is split, so it changes the level for the applications that record the
  phone microphone and for the speaker monitor at the same time.
- **Camera and microphone switches** on both sides. Either one can be switched off
  without touching the other, and a microphone-only session never powers the camera.

### Changed

- **Desktop interface**: new monochrome design with Phosphor icons, a pairing screen with
  the address, the PIN in separate boxes and the QR code in a white card, and a connected
  view with the camera and microphone tiles, the camera settings, the virtual camera, the
  audio output and the noise suppression. The scroll bar no longer covers the controls.
- **Phone interface**: the camera and the microphone have their own card with a switch
  and a state line, the preview follows the screen orientation and is never stretched,
  and a card with the camera settings is hidden while the camera is off. The pairing
  screen and the controls share the same layout in portrait and in landscape.
- **Virtual camera** is now called **ReCast Camera**, the older entries are removed when
  it is registered, and the filter no longer crashes when it is opened before the phone
  sends a picture.
- **Voice delay**: the audio queue converts the sample rate to the rate of the sound card
  and stays short, so the voice no longer lags behind the mouth.
- **QR scanner**: the code is read from a still picture instead of the preview, which
  works on every phone, and the picture is no longer distorted.

### Fixed

- The phone preview was stretched and turned the wrong way in landscape.
- The QR scanner never found a code on some phones.
- A second copy of the app closed itself instead of opening a window that could not
  connect.

## [1.0.0] - 2026-11-16

The first public release.

### Added

- **Desktop app** (`apps/app`) in Rust with an egui interface: custom borderless window
  with its own title bar, live preview, connection screen, camera and resolution
  selection, frame rate, phone microphone, torch, zoom, manual rotation, mirrored
  preview, virtual camera and virtual microphone controls, speaker monitor and
  per-device levels.
- **Phone app** (`android`) in Kotlin: connection-first screen, camera and resolution
  selection, microphone and torch switches, zoom slider, and an upright, never
  stretched preview that follows the orientation of the phone.
- **QR pairing** on both apps: the desktop shows a QR code before anything else, and the
  phone has a built-in offline scanner (Camera2 with the ZXing core) that reads it,
  fills in the address and PIN and connects. The code carries every local PC address,
  so the phone tries each one and a VPN or virtual adapter cannot block pairing. The
  same `recast://pair` link also works as a deep link.
- **ReCast identity**: new black-and-white icon used by the desktop app, the launcher
  and the Android app, all generated from `assets/recast-icon.jpg` with
  `tools/make-icons.py`; the launcher, CLI, window title, virtual camera name and
  install folder all use the ReCast name.
- **Virtual camera** filter based on the MIT-licensed softcam project, rebuilt with a
  dedicated CLSID (`{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}`) and the old CLSID kept
  as an alias, shipped for 32-bit and 64-bit applications.
- **Virtual microphone** output through any virtual audio cable, with automatic
  detection of the common ones.
- **RNNoise** noise suppression with an adjustable strength (80 % by default).
- **Single-file launcher** (`desktop/ReCast.exe`) that embeds the runtime, installs
  itself into `%LOCALAPPDATA%\ReCast`, registers the virtual camera for the current
  user, offers a one-time elevated setup for the firewall rule and the removal of an
  old camera entry, and keeps a single instance.
- **Connection over USB or Wi-Fi** with a 6-digit pairing PIN, plus a one-click *USB
  setup* that installs the APK, opens an `adb reverse` tunnel and starts the phone app.
- **Minimal LGPL FFmpeg build** (2.5 MB) used as a separate executable to decode the
  H.264 stream.
- Monochrome, Graphite, Light and custom themes.
- Build and release tooling: `tools/build-release.ps1`, the FFmpeg build script, the
  camera filter sources and the release workflow.

### Privacy

- No accounts, no advertisements, no analytics and no crash reporting.
- Video and audio travel directly between the phone and the PC and never leave the
  local network or the USB link.

[1.0.0]: https://github.com/xxRelaXxx/ReCast/releases/tag/v1.0.0
