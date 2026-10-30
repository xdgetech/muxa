#!/usr/bin/env bash
# Checks the store listings in store/ against each store's limits: no file empty, no
# field too long, and no listing text that reads as a trial ("demo", "trial"), which
# App Review guideline 2.2 rejects. The review notes may say "demo"; they are not listed.
#
# Usage: scripts/check-store-text.sh [store dir]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
store="${1:-$root/store}"
# Lengths are in characters, not bytes: Japanese text needs a UTF-8 locale.
LC_ALL="$(locale -a 2>/dev/null | grep -i -m1 -E '^(c|en_us)\.utf-?8$' || echo C.UTF-8)"
export LC_ALL

failures=0
fail() {
  echo "store text: $1" >&2
  failures=$((failures + 1))
}

# <file> <limit>: the file exists, is not empty, and fits.
check() {
  local file="$1" limit="$2" text
  if [ ! -f "$file" ]; then
    fail "${file#"$store"/} is missing"
    return
  fi
  text="$(cat "$file")"
  if [ -z "${text//[[:space:]]/}" ]; then
    fail "${file#"$store"/} is empty"
  elif [ "${#text}" -gt "$limit" ]; then
    fail "${file#"$store"/} has ${#text} characters, more than $limit"
  fi
}

# Shown on the store page: never "demo" or "trial", in either language.
listed() {
  local file
  for file in "$@"; do
    [ -f "$file" ] || continue
    if grep -i -q -E '(^|[^[:alpha:]])(demo|trial)s?([^[:alpha:]]|$)|デモ|体験版|試用' "$file"; then
      fail "${file#"$store"/} says demo or trial; call it a sample workspace"
    fi
  done
}

for locale in en-US ja; do
  dir="$store/apple/$locale"
  check "$dir/name.txt" 30
  check "$dir/subtitle.txt" 30
  check "$dir/promotional_text.txt" 170
  check "$dir/description.txt" 4000
  check "$dir/keywords.txt" 100
  check "$dir/release_notes.txt" 4000
  check "$dir/support_url.txt" 255
  check "$dir/marketing_url.txt" 255
  check "$dir/privacy_url.txt" 255
  listed "$dir"/*.txt
done
check "$store/apple/review_information/notes.txt" 4000

for locale in en-US ja-JP; do
  dir="$store/play/$locale"
  check "$dir/title.txt" 30
  check "$dir/short_description.txt" 80
  check "$dir/full_description.txt" 4000
  for log in "$dir"/changelogs/*.txt; do
    check "$log" 500
  done
  listed "$dir"/*.txt "$dir"/changelogs/*.txt
done
check "$store/play/app_access.txt" 4000

if [ "$failures" -gt 0 ]; then
  exit 1
fi
echo "store text: ok"
