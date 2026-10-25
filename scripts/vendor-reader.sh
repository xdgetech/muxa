#!/usr/bin/env bash
# Fetches the Reader's JavaScript at the pinned versions and checks each file's
# sha256. The files are committed: run this only to update a version (then
# update the hashes too). Builds and the app never fetch anything.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
dest="$here/../apple/Muxa/Muxa/Reader/vendor"
base="https://cdn.jsdelivr.net/npm"

# published path | file name here | sha256
files=(
  "marked@15.0.12/marked.min.js|marked.min.js|3e7e7d7feb3e5d58cb6c804f68ab5c24cc7e5eb6270fd6e5cbb9124739217d0c"
  "marked@15.0.12/LICENSE.md|marked.LICENSE.md|8e3a3f82f59a60958f56ca08f445647c32a4733dc7ca6c2c46f6eb898471ab9c"
  "mermaid@11.6.0/dist/mermaid.min.js|mermaid.min.js|3a93016a73dc82ba890d919f9bbb176f3da9d98341650c0b517f2595cc68fef8"
  "mermaid@11.6.0/LICENSE|mermaid.LICENSE|ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80"
  "dompurify@3.2.6/dist/purify.min.js|purify.min.js|89e1fa7647cb495370d3a997ace4387f5d15d9f4c5af12352c53daa400956287"
  "dompurify@3.2.6/LICENSE|purify.LICENSE|1b02e03c3fb4f87d476c128f0eb9def1f5a1709d28b180465228bd41574623b7"
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$dest"
for entry in "${files[@]}"; do
  IFS='|' read -r url name sum <<<"$entry"
  curl -fsSL "$base/$url" -o "$work/$name"
  got="$(shasum -a 256 "$work/$name" | cut -d' ' -f1)"
  if [ "$got" != "$sum" ]; then
    echo "vendor-reader: $url has sha256 $got, expected $sum" >&2
    exit 1
  fi
  mv "$work/$name" "$dest/$name"
  echo "ok $url"
done
