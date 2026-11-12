# Changelog

All notable changes to Rewebcam are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-11-12

The first public release.

### Added

- **Desktop app** (`apps/desktop`) in Rust with an egui interface: live preview,
  connection panel, camera and resolution selection, frame rate, phone microphone, torch,
  zoom, manual rotation, mirrored preview, virtual camera and virtual microphone
  controls, speaker monitor and per-device levels.
- **Phone app** (`android`) in Kotlin: camera and resolution selection, microphone and
  torch switches, zoom slider, and an upright, never stretched preview that follows the
  orientation of the phone.
- **Virtual camera** filter based on the MIT-licensed softcam project, rebuilt with a
  dedicated CLSID (`{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}`) and the old CLSID kept
  as an alias, shipped for 32-bit and 64-bit applications.
- **Virtual microphone** output through any virtual audio cable, with automatic
  detection of the common ones.
- **RNNoise** noise suppression with an adjustable strength (80 % by default).
- **Single-file launcher** (`desktop/Rewebcam.exe`) that embeds the runtime, installs
  itself into `%LOCALAPPDATA%\Rewebcam`, registers the virtual camera for the current
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

[1.0.0]: https://github.com/xxRelaXxx/ReWebCam/releases/tag/v1.0.0
