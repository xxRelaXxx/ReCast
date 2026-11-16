#!/usr/bin/env bash
#
# Rebuilds the minimal LGPL FFmpeg build that ReCast uses to decode the phone video.
#
# Run it from an MSYS2 UCRT64 shell:
#
#   pacman -S --needed mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-nasm make
#   cd tools/ffmpeg
#   ./build.sh
#
# The result is copied to ./ffmpeg.exe.

set -euo pipefail

VERSION="9.0.2"
TARBALL="ffmpeg-${VERSION}.tar.xz"
URL="https://ffmpeg.org/releases/${TARBALL}"
SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"

here="$(cd "$(dirname "$0")" && pwd)"
build="${here}/build"

mkdir -p "${build}"
cd "${build}"

if [ ! -f "${TARBALL}" ]; then
    echo "Downloading ${URL}"
    if command -v curl >/dev/null 2>&1; then
        curl -L --fail -o "${TARBALL}" "${URL}"
    else
        wget -O "${TARBALL}" "${URL}"
    fi
fi

echo "Checking the checksum"
echo "${SHA256}  ${TARBALL}" | sha256sum -c -

if [ ! -d "ffmpeg-${VERSION}" ]; then
    tar -xf "${TARBALL}"
fi

cd "ffmpeg-${VERSION}"

echo "Configuring"
./configure \
    --prefix="${build}/out" \
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

echo "Building"
make -j"$(nproc 2>/dev/null || echo 4)"

cp -f ffmpeg.exe "${here}/ffmpeg.exe"

echo
echo "Done: ${here}/ffmpeg.exe"
"${here}/ffmpeg.exe" -hide_banner -version | head -n 3
sha256sum "${here}/ffmpeg.exe"
