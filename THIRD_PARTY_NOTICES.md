# Third-party notices

Rewebcam itself is released under the MIT licence (see [LICENSE](LICENSE)). It bundles
and links the following third-party components. Each component keeps its own licence.

---

## 1. Bundled executables and libraries

### FFmpeg 9.0.2 — LGPL-2.1-or-later

`tools/ffmpeg/ffmpeg.exe` (also embedded in `desktop/Rewebcam.exe`) is the official
FFmpeg 9.0.2 source, compiled as a **separate executable** with only the components
Rewebcam needs. It contains **no GPL code**: `--enable-gpl` and `--enable-nonfree` were
not used, so the result is licensed under LGPL-2.1-or-later.

* Upstream project: <https://ffmpeg.org/>
* Source of the exact build: `ffmpeg-9.0.2.tar.xz`
  * <https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz>
  * SHA-256: `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`
* The same tarball is attached to every GitHub release as `ffmpeg-9.0.2.tar.xz`.
* The licence texts are in `tools/ffmpeg/licenses/` (`COPYING.LGPLv2.1`,
  `COPYING.LGPLv3`, `LICENSE.md`, plus the GPL texts for reference).
* The configure line used for the build is recorded in `tools/ffmpeg/README.md` and in
  `tools/ffmpeg/build.sh`.

FFmpeg is used as an external process: Rewebcam writes the H.264 stream to its standard
input and reads decoded raw video from its standard output. The two programs remain
independent works, which is why the LGPL obligations are met by shipping the source
alongside the executable.

### softcam 1.8.1 — MIT

The virtual camera filter (`tools/vcam/x64/softcam.dll`, `tools/vcam/x86/softcam.dll`) is
a fork of the softcam project by tshino. The full source of the fork is included in
`tools/vcam/source`, and the complete patch against upstream v1.8.1 is in
`tools/vcam/rewebcam-softcam.patch`.

* Upstream project: <https://github.com/tshino/softcam>
* Licence file of the fork: `tools/vcam/source/LICENSE`

```text
MIT License

Copyright (c) 2020 tshino

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### RNNoise — BSD-3-Clause

The noise suppression uses the Rust binding `nnnoiseless`, which wraps the RNNoise
library by Xiph.Org.

* Upstream project: <https://github.com/xiph/rnnoise>
* Copyright: 2016 Xiph.Org Foundation

```text
Copyright (c) 2016, Xiph.Org Foundation
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright notice, this
  list of conditions and the following disclaimer.
* Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.
* Neither the name of the Xiph.Org Foundation nor the names of its
  contributors may be used to endorse or promote products derived from this
  software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```

---

## 2. Rust crates

The desktop app and the launcher are Rust programs. The complete list of the 452 crates
that are compiled into them, with their versions and their declared licences, is in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). The exact versions are pinned in
`Cargo.lock`.

The licences that occur are MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib,
Unicode-3.0, 0BSD, Unlicense, BSL-1.0, CC0-1.0, BlueOak-1.0, the Apache-2.0 LLVM
exception, the SIL Open Font License and the Ubuntu Font Licence. All of them are
permissive and compatible with the MIT licence of this project.

The full text of the two most common ones is reproduced below for convenience; the
others are published with the crates themselves and are linked from
`THIRD_PARTY_LICENSES.md`.

### MIT (as used by 99+ crates)

```text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License 2.0

The Apache License 2.0 applies to parts of the dependency tree (for example `wgpu`,
`winit` and `naga`). Its full text is published at
<https://www.apache.org/licenses/LICENSE-2.0> and is included in the source
distribution of every affected crate.

---

## 3. Fonts

The desktop application embeds the default fonts of the egui project:

| Font | Licence | Text |
| --- | --- | --- |
| Hack Regular | MIT | `licenses/fonts/Hack-Regular-LICENSE.txt` |
| Ubuntu Light | Ubuntu Font Licence 1.0 | `licenses/fonts/Ubuntu-Font-Licence-1.0.txt` |
| Noto Emoji Regular | SIL Open Font License 1.1 | `licenses/fonts/NotoEmoji-OFL-1.1.txt` |
| emoji-icon-font | MIT | `licenses/fonts/emoji-icon-font-LICENSE.txt` |

---

## 4. Android

The phone app is written in Kotlin and uses the Kotlin standard library, which is
distributed under the Apache License 2.0:

* <https://github.com/JetBrains/kotlin> — Apache-2.0

The app uses only the Android platform APIs (Camera2, MediaCodec, AudioRecord) and has no
third-party runtime dependencies.

---

## 5. Other components referenced by the project

These are not shipped as binaries; they are credited because the design or parts of the
implementation follow their documentation and public interfaces.

| Component | Licence | Role |
| --- | --- | --- |
| [egui](https://github.com/emilk/egui) | MIT / Apache-2.0 | the desktop user interface |
| [eframe](https://github.com/emilk/egui) | MIT / Apache-2.0 | the desktop application framework |
| [cpal](https://github.com/RustAudio/cpal) | MIT | the audio output used for the virtual microphone and the monitor |
| [libloading](https://github.com/nomic-ai/libloading) | MIT / Apache-2.0 | loading the camera filter at run time |
| [Voice.ai Audio Cable](https://voice.ai/) | freeware | the recommended virtual audio cable (not redistributed) |
