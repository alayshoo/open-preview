#!/usr/bin/env bash
# Cross-builds open-preview.exe and packages dist/Open-Preview-<version>-x64.msi from Linux.
# Needs: rustup target x86_64-pc-windows-gnu, mingw-w64, msitools (wixl), 7z, curl.
# Usage: scripts/build-msi.sh [path/to/libmpv-2.dll]
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
stage=target/msi-stage
rm -rf "$stage" && mkdir -p "$stage" dist

cargo build --release --target x86_64-pc-windows-gnu -p open-preview -p open-preview-thumbs

dll=${1:-}
if [[ -z "$dll" ]]; then
    # Latest baseline x86_64 build from the official SourceForge mirror.
    rss=$(curl -fsSL --retry 3 "https://sourceforge.net/projects/mpv-player-windows/rss?path=/libmpv")
    name=$(grep -oE 'mpv-dev-x86_64-[0-9]{8}-git-[0-9a-f]+\.7z' <<<"$rss" | head -1)
    dl=target/libmpv/$name
    mkdir -p target/libmpv
    [[ -f "$dl" ]] || curl -fsSL --retry 3 -o "$dl" "https://sourceforge.net/projects/mpv-player-windows/files/libmpv/$name/download"
    7z e -y -otarget/libmpv "$dl" libmpv-2.dll >/dev/null
    dll=target/libmpv/libmpv-2.dll
fi

cp target/x86_64-pc-windows-gnu/release/open-preview.exe target/x86_64-pc-windows-gnu/release/open_preview_thumbs.dll assets/open-preview.ico installer/THIRD-PARTY-NOTICES.txt "$dll" "$stage/"
wixl -a x64 -D Version="$version" -D BinDir="$stage" -o "dist/Open-Preview-$version-x64.msi" installer/open-preview.wxs
sha256sum "dist/Open-Preview-$version-x64.msi"
