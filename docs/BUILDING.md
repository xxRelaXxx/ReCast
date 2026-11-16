# Building ReCast

Everything in this repository can be rebuilt from source. The release binaries are
committed so that the project can be used without a build toolchain, but the sources are
the reference.

---

## 1. Prerequisites

| Tool | Needed for | Version used here |
| --- | --- | --- |
| Rust (MSVC toolchain) | desktop app and launcher | 1.99.1 |
| Visual Studio 2022 / Build Tools with the *Desktop development with C++* workload | linking the Rust binaries and building the camera filter | 17.x |
| Android SDK (platform 34, build-tools 34) + JDK | phone app | JDK 21 (17 also works) |
| MSYS2 UCRT64 with `mingw-w64-ucrt-x86_64-gcc` and `nasm` | rebuilding FFmpeg | GCC 16.2, NASM 2.16 |

Optional but handy: `adb` from the Android platform-tools for the one-click USB setup, and
a virtual audio cable for testing the microphone path.

Check the toolchain with:

```powershell
rustc -vV
cargo -V
java -version
```

---

## 2. The desktop app

```powershell
cargo build --release -p recast-app
```

The result is `target\release\recast-app.exe`. It is not the file you ship: the shipped
file is the launcher, which embeds this binary.

Useful environment variables while developing:

| Variable | Effect |
| --- | --- |
| `RECAST_FFMPEG` | Path to the FFmpeg executable to use. |
| `RECAST_SOFTCAM_DLL` | Path to the camera filter to load. |
| `RECAST_ADB` | Path to `adb.exe`. |
| `RECAST_PIN` | Fix the pairing PIN instead of generating a random one. |
| `RECAST_DUMP_STREAM` | Write the received H.264 stream to a file. |

### Icons

The artwork lives in `assets/recast-icon.jpg`. Every platform icon is generated from it:

```powershell
python -m pip install pillow
python tools\make-icons.py
```

The script writes the multi-size `.ico` and PNG used by the app and the launcher, the raw
RGBA window icon, and the Android launcher, round and adaptive-foreground mipmaps. Run it
after replacing the source artwork.

---

## 3. The phone app

```powershell
cd android
.\gradlew.bat assembleRelease
```

The APK is written to `android\app\build\outputs\apk\release\app-release.apk`.

The release build is signed with the local Android debug key on purpose: it makes the APK
sideloadable without extra tooling. Android debug keys are per-machine, so a release built
on another machine has a different signature and cannot upgrade an installed copy — you
have to uninstall the old app first. For a store release, replace the signing config in
`android\app\build.gradle.kts` with your own key.

The SDK location is read from `android\local.properties` (`sdk.dir=...`), which is not
committed. Either create that file or set `ANDROID_HOME`.

---

## 4. FFmpeg

ReCast decodes the phone video with a separate FFmpeg executable. The committed binary
is a minimal build: H.264 in, raw video out, and only the `transpose`, `hflip`, `vflip`,
`scale`, `format` and `null` filters. It is 2.5 MB instead of the 100 MB of a general
purpose build.

It is built from the official FFmpeg 9.0.2 sources with the GPL components disabled, so
the result is LGPL-2.1-or-later.

```bash
# MSYS2 UCRT64 shell
pacman -S --needed mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-nasm make
cd tools/ffmpeg
./build.sh
```

`tools/ffmpeg/build.sh` downloads the official tarball, verifies its SHA-256 checksum and
applies the configure line recorded in `tools/ffmpeg/README.md`. The result is copied to
`tools/ffmpeg/ffmpeg.exe`.

---

## 5. The virtual camera filter

The filter is a C++ DirectShow source built with MSBuild:

```powershell
msbuild tools\vcam\source\softcam.sln /p:Configuration=Release /p:Platform=x64
msbuild tools\vcam\source\softcam.sln /p:Configuration=Release /p:Platform=Win32
```

The DLLs appear in `tools\vcam\source\x64\Release\softcam.dll` and
`tools\vcam\source\Win32\Release\softcam.dll`. Copy them to `tools\vcam\x64\softcam.dll`
and `tools\vcam\x86\softcam.dll`.

The source is a fork of [softcam](https://github.com/tshino/softcam) v1.8.1 (MIT). The
complete patch against upstream is in `tools/vcam/recast-softcam.patch`, so the changes
can be reviewed or re-applied to a newer upstream release.

---

## 6. The launcher and the release archive

`tools/build-release.ps1` puts everything together:

```powershell
.\tools\build-release.ps1
```

It

1. builds the desktop app (`cargo build --release -p recast-app`);
2. builds the phone app (`gradlew assembleRelease`), unless `-SkipAndroid` is given;
3. checks that `tools\ffmpeg\ffmpeg.exe` and both camera DLLs are present;
4. stages the runtime into `apps\launcher\payload`;
5. builds the launcher (`cargo build --release --manifest-path apps\launcher\Cargo.toml`);
6. copies the launcher to `desktop\ReCast.exe` and the APK to `phone\ReCastPhone.apk`;
7. writes the SHA-256 checksums and creates `release\ReCast-<version>-win64.zip`.

Useful switches:

| Switch | Effect |
| --- | --- |
| `-SkipAndroid` | Reuse the APK that is already committed. |
| `-SkipLauncher` | Only rebuild the desktop app and the APK. |
| `-Version 1.0.1` | The version written into the archive name and the checksum file. |
| `-NoZip` | Do not create the archive. |

The launcher payload (`apps\launcher\payload`) is generated and not committed; the
launcher embeds it with `include_bytes!`, so it must exist before the launcher is built.

### Version numbers

Five files carry the version and must be kept in sync:

| File | Entry |
| --- | --- |
| `apps/app/Cargo.toml` | `version = "1.0.0"` |
| `apps/cli/Cargo.toml` | `version = "1.0.0"` |
| `apps/core/Cargo.toml` | `version = "1.0.0"` |
| `apps/launcher/Cargo.toml` | `version = "1.0.0"` |
| `android/app/build.gradle.kts` | `versionName = "1.0.0"` |

`tools/build-release.ps1` refuses to build when these versions differ.

---

## 7. Releasing

1. Update the three version numbers and `CHANGELOG.md`.
2. Run `.\tools\build-release.ps1` and commit the rebuilt `desktop\ReCast.exe`,
   `phone\ReCastPhone.apk` and the launcher payload changes.
3. Tag and push:

   ```powershell
   git tag v1.0.0
   git push origin main --tags
   ```

4. The `release` workflow (`.github/workflows/release.yml`) picks the tag up, builds
   `ReCast-win64.zip`, downloads the matching FFmpeg source, writes the checksum file
   and publishes everything as a GitHub release.

The workflow does not compile anything: it packages the committed binaries. Always run
`tools/build-release.ps1` before tagging.

---

## 8. Repository layout

```text
apps/app          Rust desktop application
apps/cli          Rust command line tool
apps/core         Shared Rust library
apps/launcher     Rust one-file launcher and installer
android/          Kotlin phone application
desktop/          The shipped launcher (one file)
phone/            The shipped APK (one file)
assets/           Icon source artwork
docs/             User guide, architecture notes, build notes, screenshots
tools/make-icons.py  Regenerates the desktop and Android icons
tools/ffmpeg      Minimal LGPL FFmpeg build, licences and build script
tools/vcam        Virtual camera filter: binaries, source, patch, build notes
tools/            build-release.ps1 and other helper scripts
.github/          CI and release workflows, issue templates
```
