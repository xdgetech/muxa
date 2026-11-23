#!/usr/bin/env bash
# Fetches the terminal's fonts at the pinned versions and checks each file's
# sha256: JetBrains Mono (Ghostty's default font, SIL OFL 1.1) and the Nerd
# Fonts symbols, which draw the icons prompts and TUIs print (MIT). The files
# are committed: run this only to update a version (then update the hashes
# too). Builds and the apps never fetch anything.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
dest="$here/../apple/Muxa/Muxa/Fonts"
base="https://cdn.jsdelivr.net/gh"
jbm="JetBrains/JetBrainsMono@v2.304"
nerd="ryanoasis/nerd-fonts@v3.4.0/patched-fonts/NerdFontsSymbolsOnly"

# published path | file name here | sha256
files=(
  "$jbm/fonts/ttf/JetBrainsMono-Regular.ttf|JetBrainsMono-Regular.ttf|a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f"
  "$jbm/fonts/ttf/JetBrainsMono-Bold.ttf|JetBrainsMono-Bold.ttf|5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb"
  "$jbm/fonts/ttf/JetBrainsMono-Italic.ttf|JetBrainsMono-Italic.ttf|9d0a1f7a708e6af183f1193b7e81d40da294f5c67682c085d8401c60aac8ded4"
  "$jbm/fonts/ttf/JetBrainsMono-BoldItalic.ttf|JetBrainsMono-BoldItalic.ttf|4039d5ce0ed225bf9c8b2c8c6436290ae2f356b7e90d70fa666227238324aa3b"
  "$jbm/OFL.txt|JetBrainsMono.OFL.txt|30f0c136e3c88e422d0791acd97238870f9054a9729bc34cf2ff0d4ed8cac4ad"
  "$nerd/SymbolsNerdFontMono-Regular.ttf|SymbolsNerdFontMono-Regular.ttf|f0f624d9b474bea1662cf7e862d44aebe1ae1f6c7f9cb7a0ca5d0e5ac9561c60"
  "$nerd/LICENSE|SymbolsNerdFont.LICENSE|84a7a98c82140fb12c37fe42b93805baa16024cb3e5acc599b7ffe612c55d847"
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$dest"
for entry in "${files[@]}"; do
  IFS='|' read -r url name sum <<<"$entry"
  curl -fsSL "$base/$url" -o "$work/$name"
  got="$(shasum -a 256 "$work/$name" | cut -d' ' -f1)"
  if [ "$got" != "$sum" ]; then
    echo "vendor-fonts: $url has sha256 $got, expected $sum" >&2
    exit 1
  fi
  mv "$work/$name" "$dest/$name"
  echo "ok $url"
done
