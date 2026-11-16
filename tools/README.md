# tools

Helper scripts and the third-party binaries that ReCast ships.

| Path | What it is |
| --- | --- |
| `build-release.ps1` | Builds the desktop app, the phone app and the launcher, and creates the release archive. |
| `ffmpeg/` | The minimal LGPL FFmpeg build used to decode the phone video, its licences and `build.sh` to rebuild it. |
| `vcam/` | The virtual camera filter: the x64 and x86 DLLs, the complete source of the fork and the patch against upstream softcam. |

Start with:

```powershell
.\tools\build-release.ps1
```

The script needs a Rust toolchain and, unless `-SkipAndroid` is used, the Android SDK and
a JDK. The detailed build instructions are in [../docs/BUILDING.md](../docs/BUILDING.md).

The release workflow in `.github/workflows/release.yml` does **not** compile anything: it
packages the committed binaries in `desktop/` and `phone/`. Always run
`tools/build-release.ps1` before tagging a release.
