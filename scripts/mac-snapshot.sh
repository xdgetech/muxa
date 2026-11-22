#!/bin/bash
# Draws the debug Mac app's demo window into a PNG: scripts/mac-snapshot.sh OUT.png [home|focus|routines|shortcuts]
# Needs `make build-mac-app` first. No screen-recording permission: the app draws itself.
set -euo pipefail
out="$1"
screen="${2:-home}"
app="$(xcodebuild -project apple/Muxa/Muxa.xcodeproj -scheme MuxaMac -destination 'platform=macOS' \
  CODE_SIGN_IDENTITY=- CODE_SIGN_ENTITLEMENTS= DEVELOPMENT_TEAM= -showBuildSettings 2>/dev/null |
  awk '$1 == "BUILT_PRODUCTS_DIR" { print $3 }')/Muxa.app"
rm -f "$out"
open -n --env "MUXA_SNAPSHOT=$out" --env "MUXA_SCREEN=$screen" "$app" --args -demo
for _ in $(seq 1 30); do
  [ -f "$out" ] && break
  sleep 1
done
[ -f "$out" ] || { echo "no snapshot at $out" >&2; exit 1; }
echo "$out"
