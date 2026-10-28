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

dll=${1:-$(scripts/fetch-libmpv.sh)}

cp target/x86_64-pc-windows-gnu/release/open-preview.exe target/x86_64-pc-windows-gnu/release/open_preview_thumbs.dll assets/open-preview.ico installer/THIRD-PARTY-NOTICES.txt "$dll" "$stage/"
wixl -a x64 -D Version="$version" -D BinDir="$stage" -o "dist/Open-Preview-$version-x64.msi" installer/open-preview.wxs
sha256sum "dist/Open-Preview-$version-x64.msi"
