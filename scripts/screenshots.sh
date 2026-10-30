#!/usr/bin/env bash
# Takes the App Store and README screenshots from the in-app showcase demo
# (ShowcaseTests), on simulators of their own so the status bar override and
# the run never touch the developer's simulators.
#
# Usage: scripts/screenshots.sh
# Out:   build/screenshots/appstore-ipad/{en,ja}/    captioned, 2752x2064 (App Store 13-inch iPad)
#        build/screenshots/appstore-iphone/{en,ja}/  captioned, 1320x2868 (App Store 6.9-inch iPhone)
#        build/screenshots/raw/{ipad,iphone}/        plain screens
#        docs/images/                                README images
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build/screenshots"

# <simulator name> <device type> <slot> <width> <height>: shoots, frames and checks one slot.
shoot() {
  local name="$1" type="$2" slot="$3" width="$4" height="$5" udid raw shot size
  udid="$(xcrun simctl list devices | grep -F "$name (" | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/' || true)"
  if [ -z "$udid" ]; then
    udid="$(xcrun simctl create "$name" "$type")"
  fi
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl status_bar "$udid" override --time "9:41" --batteryState charged --batteryLevel 100 \
    --wifiBars 3 --dataNetwork wifi

  raw="$out/raw/$slot"
  rm -rf "$raw" "$out/appstore-$slot"
  mkdir -p "$raw"
  TEST_RUNNER_MUXA_SHOWCASE_DIR="$raw" xcodebuild -project "$root/apple/Muxa/Muxa.xcodeproj" -scheme Muxa \
    -skipPackagePluginValidation -destination "platform=iOS Simulator,id=$udid" \
    -only-testing:MuxaUITests/ShowcaseTests test >"$out/xcodebuild-$slot.log" 2>&1 || {
    echo "ShowcaseTests failed on $type; see $out/xcodebuild-$slot.log" >&2
    xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true
    exit 1
  }
  # The device is kept for the next run, but not left running.
  xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true

  swift "$root/apple/Muxa/Art/frame-screenshots.swift" "$slot" "$raw" "$out/appstore-$slot"
  # The App Store rejects any other size.
  for shot in "$out/appstore-$slot"/*/*.png; do
    size="$(sips -g pixelWidth -g pixelHeight "$shot" | awk '/pixel/ { printf "%s ", $2 }')"
    if [ "$size" != "$width $height " ]; then
      echo "$shot is $size, not ${width}x$height" >&2
      exit 1
    fi
  done
}

mkdir -p "$out"
make -C "$root" app-project >/dev/null
shoot muxa-screenshots "iPad Pro 13-inch (M5)" ipad 2752 2064
shoot muxa-screenshots-iphone "iPhone 18 Pro Max" iphone 1320 2868

mkdir -p "$root/docs/images"
for name in 1-chat 2-agent 3-switcher 4-plan; do
  sips -Z 1600 -s format jpeg -s formatOptions 80 "$out/raw/ipad/$name.png" \
    --out "$root/docs/images/${name#*-}.jpg" >/dev/null
done
echo "App Store: $out/appstore-ipad, $out/appstore-iphone"
echo "README:    $root/docs/images"
