# Architecture

Rewebcam is three small programs and one shared protocol.

```text
┌──────────────────────────┐        ┌───────────────────────────────────────────┐
│ Android phone            │        │ Windows PC                                 │
│                          │        │                                           │
│ CameraController         │        │ apps/launcher  Rewebcam.exe                │
│  └ Camera2 + MediaCodec  │        │   ├ unpacks the runtime                   │
│     (H.264, 1280x720/    │        │   ├ registers the virtual camera (HKCU)   │
│      1920x1080 @ 30 fps) │        │   └ one-time elevated setup               │
│                          │        │                                           │
│ AudioCapture             │  TCP  │ apps/desktop  rewebcam.exe (egui)         │
│  └ AudioRecord           │  47777 │   ├ session, protocol, statistics         │
│     (48 kHz mono PCM)    │ ─────► │   ├ FFmpeg child process (H.264 → RGB)     │
│                          │        │   ├ RNNoise on the audio path              │
│ StreamController         │        │   └ softcam.dll  scSendFrame(...)         │
│  └ Camera2 + AudioRecord │        │       │                                   │
└──────────────────────────┘        │       ▼                                   │
                                    │  "Rewebcam Virtual Camera" (DirectShow)    │
                                    │       │                                   │
                                    │       ▼                                   │
                                    │  Zoom, Discord, OBS, browsers, ...         │
                                    └───────────────────────────────────────────┘
```

---

## Repository layout

| Path | What it is |
| --- | --- |
| `apps/desktop` | The desktop app: `eframe`/`egui` interface, session handling, decoder bridge, audio output, virtual camera control. |
| `apps/launcher` | The one-file launcher `Rewebcam.exe`: unpacks the runtime, registers the camera, offers the elevated setup, starts the app, and keeps a single instance. |
| `android` | The phone app: Kotlin, `Camera2`, `MediaCodec` and `AudioRecord`. |
| `tools/ffmpeg` | The minimal LGPL FFmpeg build used to decode the video, plus its licences and build script. |
| `tools/vcam` | The virtual camera filter (a fork of softcam), the prebuilt x64/x86 DLLs, the full source and the patch against upstream. |
| `tools/build-release.ps1` | Builds everything and produces the release archive. |

---

## The link between the phone and the PC

One TCP connection on port **47777**. Every message is a 20-byte header followed by a
payload:

```text
 0        4      5      6        8           12                20
 +--------+------+------+--------+-----------+------------------+---------+
 | "RWBC" | type | flag | 0 (rsv)|  length   |  pts (µs, u64)  | payload |
 +--------+------+------+--------+-----------+------------------+---------+
```

| Type | Name | Direction | Payload |
| --- | --- | --- | --- |
| 1 | `HELLO` | phone → PC | `{"pin": "123456", "device": "Pixel 7", "version": 1}` |
| 2 | `HELLO_ACK` | PC → phone | `{"ok": true, "message": "Connected"}` |
| 3 | `CAPS` | phone → PC | camera list, supported resolutions, current selection |
| 4 | `VIDEO_CONFIG` | phone → PC | `{"codec": "h264", "width": 1280, "height": 720, "fps": 30, "rotation": 0, "mirror": false}` |
| 5 | `VIDEO` | phone → PC | raw H.264 access units |
| 6 | `AUDIO` | phone → PC | 16-bit little-endian PCM, 48 kHz mono |
| 7 | `CONTROL` | both | `{"cmd": "set_camera" | "set_resolution" | "set_mic" | "set_torch" | "set_zoom" | "start" | "stop"}` |
| 8 | `STATS` | phone → PC | frame rate, bitrate, battery, thermal state, current selection |
| 9 / 10 | `PING` / `PONG` | both | round-trip measurement for the latency readout |
| 11 | `ERROR` | both | `{"message": "..."}` |

The desktop app sends `CONTROL {"cmd":"start"}` as soon as the PIN is accepted. The phone
then starts the encoder and the microphone.

Both sides keep their own state; the `STATS` frames let the desktop app show changes that
were made on the phone, and the `CONTROL` frames do the same in the other direction. A
short quiet period (1.8 s) after a control message prevents the two sides from fighting
over the same setting.

---

## Video path

1. The phone encodes with the hardware H.264 encoder in a low-latency configuration
   (small GOP, no B-frames) and writes the access units into the TCP stream.
2. The desktop app pipes the H.264 bytes into `ffmpeg.exe`:

   ```text
   ffmpeg -hide_banner -loglevel error -fflags nobuffer -flags low_delay
          -probesize 32 -analyzeduration 0 -f h264 -i pipe:0 -an
          [-vf transpose=1|hflip,vflip|transpose=2[,hflip]]
          -fps_mode passthrough -f rawvideo -pix_fmt bgr24 -flush_packets 1 pipe:1
   ```

   `-fps_mode passthrough` matters: with a tiny probe the raw H.264 demuxer cannot guess
   the frame rate and would drop almost every frame.
3. The `bgr24` frames are uploaded to the preview texture (downscaled by two when the
   stream is larger than 1280 px wide) and copied into the virtual camera filter with
   `scSendFrame`.
4. The rotation reported by the phone is applied by FFmpeg, and the manual **+90°**
   offset is added on top of it. When the result is rotated by 90°, the output size
   switches between 1920x1080 and 1080x1920.

The FFmpeg binary is a separate executable with a minimal feature set (H.264 in, raw video
out, and the `transpose`, `hflip`, `vflip` filters). It is built from the official FFmpeg
sources with `--disable-everything` and only the needed components enabled, which keeps
it at about 2.5 MB and under the LGPL.

---

## Audio path

1. The phone records 48 kHz mono PCM with `AudioRecord`.
2. The desktop app receives PCM frames and, when RNNoise is enabled, runs them through
   the noise suppressor frame by frame (480 samples per frame).
3. The result is mixed between the raw and the cleaned signal according to the strength
   setting and pushed into two sinks:
   * the **monitor** sink, played on the PC speakers when enabled;
   * the **microphone** sink, written to the selected virtual audio cable.

Both sinks are plain ring buffers filled by the CPAL output callback, so a slow or missing
audio device never blocks the network thread.

---

## The virtual camera

`tools/vcam` contains a fork of the MIT-licensed [softcam](https://github.com/tshino/softcam)
project. The filter is a DirectShow source registered under the *Video Input Device*
category, so every DirectShow and Media Foundation application sees it as a camera.

Changes against upstream v1.8.1 (see `tools/vcam/rewebcam-softcam.patch`):

* the primary CLSID was changed to `{8E2C0D5B-3F5A-4D7E-9B2C-6A1D4F7E9C31}` so that
  Rewebcam does not fight with other softcam builds over the shared memory segment;
* the old CLSID `{AEF3B972-5FA5-4647-9571-358EB472BC9E}` is registered as an alias, so
  applications that already selected the camera keep working;
* the filter, stream and shared-memory names were renamed to "Rewebcam Virtual Camera";
* the frame-buffer protocol carries a version byte so old and new builds cannot
  misinterpret each other's frames.

The launcher registers the filter under `HKCU\Software\Classes\CLSID` for the 64-bit and
the 32-bit registry views, and writes the `REGFILTER2` blob that describes the single
video output pin.

---

## The launcher

`apps/launcher` produces the single `desktop/Rewebcam.exe`:

* the runtime (`rewebcam.exe`, `ffmpeg.exe`, `RewebcamPhone.apk`, both camera DLLs) is
  embedded with `include_bytes!` and unpacked into `%LOCALAPPDATA%\Rewebcam`;
* files that are already identical are not rewritten, so an update does not fail while
  another application has the camera DLL open;
* the virtual camera is registered for the current user on every launch;
* a one-time elevated step removes an old machine-wide camera entry and adds the
  firewall rule;
* a single instance is enforced by comparing the **full path** of the running process,
  not its image name — `Rewebcam.exe` and `rewebcam.exe` are the same name to
  case-insensitive Windows, so an image-name comparison would kill the launcher itself;
* every step is appended to `%LOCALAPPDATA%\Rewebcam\launcher.log`.

---

## The phone app

* `CameraController` enumerates the cameras, their resolutions and frame rates, and
  applies the zoom and torch settings.
* `VideoEncoder` drives `MediaCodec` in a low-latency H.264 configuration.
* `AudioCapture` reads 48 kHz mono PCM from `AudioRecord`.
* `StreamController` owns the foreground service, the connection and the frame queue. Video
  frames are dropped instead of blocking the camera when the network is slow; control
  messages are always delivered.
* `MainActivity` renders the preview with a matrix that undoes the `TextureView` stretch,
  rotates to the current display orientation and scales the upright image so that it
  covers the view without letterboxing.

---

## License notes

* The Rust and Kotlin code in this repository is MIT licensed.
* The virtual camera filter is MIT licensed (softcam, Copyright (c) 2020 tshino).
* The bundled FFmpeg executable is LGPL-2.1-or-later: it is a separate executable built
  from the official sources with GPL components disabled. The matching source is offered
  with every release, see `tools/ffmpeg/README.md`.
* RNNoise is BSD-3-Clause.
