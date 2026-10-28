#!/usr/bin/env bash
# Downloads the latest baseline x86_64 libmpv for Windows from the official
# SourceForge mirror (shinchiro's builds, as linked from mpv.io) into
# target/libmpv, and prints the path of libmpv-2.dll. Needs: curl, 7z.
set -euo pipefail
cd "$(dirname "$0")/.."

rss=$(curl -fsSL --retry 3 "https://sourceforge.net/projects/mpv-player-windows/rss?path=/libmpv")
name=$(grep -oE 'mpv-dev-x86_64-[0-9]{8}-git-[0-9a-f]+\.7z' <<<"$rss" | sed -n 1p)
dl=target/libmpv/$name
mkdir -p target/libmpv
[[ -f "$dl" ]] || curl -fsSL --retry 3 -o "$dl" "https://sourceforge.net/projects/mpv-player-windows/files/libmpv/$name/download"
7z e -y -otarget/libmpv "$dl" libmpv-2.dll >/dev/null
echo "target/libmpv/libmpv-2.dll"
