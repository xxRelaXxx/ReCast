# Contributing to Rewebcam

Thanks for helping out. Rewebcam is a small, self-contained project: a Rust desktop app, a
Rust launcher, a Kotlin phone app and the virtual camera filter that goes with them.

## Before you start

* Please open an issue before a large change, so the design can be discussed before you
  spend an evening on it.
* Keep pull requests focused. One topic per pull request is much easier to review.
* Do not add dependencies lightly. The desktop app is meant to stay small and to start
  instantly; every crate adds to the download size and to the build time.

## Development setup

```powershell
git clone git@github.com:xxRelaXxx/ReWebCam.git
cd ReWebCam
cargo build --release -p rewebcam     # desktop app
cd android; .\gradlew.bat assembleRelease   # phone app
```

More details, including the camera filter and FFmpeg, are in
[docs/BUILDING.md](docs/BUILDING.md).

### Running the desktop app without a phone

The desktop app works without a phone; it simply waits for a connection and shows the
address and PIN. Useful environment variables while developing:

| Variable | Effect |
| --- | --- |
| `REWEBCAM_PIN` | Use a fixed pairing PIN. |
| `REWEBCAM_FFMPEG` | Use a specific FFmpeg executable. |
| `REWEBCAM_SOFTCAM_DLL` | Load a specific camera filter. |
| `REWEBCAM_DUMP_STREAM` | Write the received H.264 stream to a file. |

## Code style

* Rust: `cargo fmt` and `cargo clippy -- -D warnings` must pass without new warnings.
* Kotlin: the standard Android style, four spaces, no wildcard imports.
* Comments explain *why*, not *what*.
* User-facing strings are in English and start with a capital letter.

## Commit messages and pull requests

* One logical change per commit, with a short, imperative subject line
  (`Fix the preview rotation when the phone reports 180°`).
* Describe what a pull request changes and how it was tested. A screenshot of the
  interface is welcome.
* Update `CHANGELOG.md` under the *Unreleased* heading.

## Licensing

By contributing you agree that your work is published under the MIT licence of this
project. Third-party code must keep its own licence and be listed in
`THIRD_PARTY_NOTICES.md`.

If you contribute a change to the virtual camera filter, keep the patch against upstream
softcam small and reviewable: `tools/vcam/rewebcam-softcam.patch` is regenerated from the
fork and is part of the release documentation.

## Reporting bugs

Please include the Windows version, the phone model and Android version, how the phone
was connected (USB or Wi-Fi), what you expected and what happened instead. The log file
`%LOCALAPPDATA%\Rewebcam\launcher.log` is often helpful.
