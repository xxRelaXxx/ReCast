# FFmpeg

ReCast decodes the H.264 video that the phone sends. It does not implement a video
decoder itself: a small, separate FFmpeg executable does the decoding and the rotation.

## Why a separate build

The general purpose Windows builds of FFmpeg are around 100 MB. ReCast only needs to
turn an H.264 elementary stream into raw pixels and to rotate or mirror them, so this
directory contains a build with almost everything switched off. It is **2.5 MB** instead
of 100 MB, it starts in a few milliseconds, and it contains no code that ReCast does
not use.

Keeping FFmpeg as a separate executable also keeps the licence situation simple: the
program is a work in its own right under the LGPL, and ReCast talks to it through
pipes.

## The committed binary

| | |
| --- | --- |
| Version | FFmpeg 9.0.2 |
| Licence | LGPL-2.1-or-later (no GPL components) |
| Size | 2 660 878 bytes |
| SHA-256 | `dce58c5bd6394a3138104cb3ddb280dddc07af1cac315df748daeb2c4c0f370c` |

Configure line used for the build:

```text
./configure \
  --prefix=<build-dir>/out \
  --arch=x86_64 --target-os=mingw32 \
  --cc=gcc --cxx=g++ --windres=windres --ar=ar --ranlib=ranlib --strip=strip \
  --disable-autodetect --disable-doc --disable-ffplay --disable-ffprobe \
  --disable-network --disable-debug --disable-iconv --disable-zlib \
  --disable-bzlib --disable-lzma --disable-sdl2 --disable-xlib \
  --enable-static --disable-shared --enable-small \
  --enable-swscale --enable-avfilter \
  --disable-everything \
  --enable-ffmpeg \
  --enable-protocol=pipe \
  --enable-demuxer=h264 --enable-parser=h264 --enable-decoder=h264 \
  --enable-encoder=rawvideo --enable-muxer=rawvideo \
  --enable-filter='transpose,hflip,vflip,scale,format,null,buffersink,buffersrc'
```

`--disable-everything` before the `--enable-*` lines means that only the listed components
are compiled in. `--disable-gpl` and `--enable-nonfree` were never used, so the binary is
LGPL and can be redistributed with the corresponding source.

## Source

The build uses the official release tarball:

* <https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz>
* SHA-256: `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`

The same tarball is attached to every GitHub release as `ffmpeg-9.0.2.tar.xz`, so the
source that corresponds to the shipped binary is always available next to it.

The licence texts of FFmpeg are in `licenses/`.

## Rebuilding

```bash
# MSYS2 UCRT64 shell
pacman -S --needed mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-nasm make
./build.sh
```

`build.sh` downloads the tarball, checks the checksum, configures and builds it, and
copies the result to `ffmpeg.exe` in this directory. The reference build used
GCC 16.2.0 (MinGW-W64, x86_64-ucrt-posix-seh) and NASM 2.16.

After rebuilding, update the SHA-256 checksum in this file and in
`THIRD_PARTY_NOTICES.md`.

## How ReCast calls it

```text
ffmpeg -hide_banner -loglevel error -fflags nobuffer -flags low_delay
       -probesize 32 -analyzeduration 0 -f h264 -i pipe:0 -an
       [-vf transpose=1 | hflip,vflip | transpose=2]
       -fps_mode passthrough -f rawvideo -pix_fmt bgr24 -flush_packets 1 pipe:1
```

The phone's H.264 access units are written to the standard input, and the decoded BGR
frames are read from the standard output.
