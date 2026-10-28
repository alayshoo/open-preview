#!/usr/bin/env bash
# Runs the media regression tests (src/tests/) twice: natively against the
# system libmpv, OpenGL path included, which also makes the test clips; then
# cross-built for Windows under Wine against the libmpv-2.dll the installer
# ships, on the same clips.
# Needs: ffmpeg, libmpv (libmpv-dev), Mesa EGL (libegl-mesa0, libgl1-mesa-dri),
# wine64, mingw-w64, 7z, curl.
# Usage: scripts/test-media.sh [path/to/libmpv-2.dll] [-- test filter and libtest options]
set -euo pipefail
cd "$(dirname "$0")/.."

dll=
if [[ $# -gt 0 && $1 != -- ]]; then
    dll=$1
    shift
fi
[[ ${1:-} == -- ]] && shift

export OPEN_PREVIEW_MEDIA_TESTS=require

echo "== System libmpv"
cargo test -- "$@"

echo "== Bundled libmpv-2.dll, under Wine"
dll=${dll:-$(scripts/fetch-libmpv.sh)}
export WINEDEBUG=-all
# No Mono or Gecko install prompts while Wine sets up its prefix.
export WINEDLLOVERRIDES="mscoree,mshtml="
OPEN_PREVIEW_LIBMPV=$(winepath -w "$(realpath "$dll")" 2>/dev/null)
export OPEN_PREVIEW_LIBMPV
# Wine can't run Linux's ffmpeg: the clips are the ones made above.
export OPEN_PREVIEW_NO_FFMPEG=1
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine cargo test --target x86_64-pc-windows-gnu -- "$@"
