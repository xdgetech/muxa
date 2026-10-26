#!/usr/bin/env bash
# Takes the App Store and README screenshots from the in-app showcase demo
# (ShowcaseTests), on a simulator of its own so the status bar override and
# the run never touch the developer's simulators.
#
# Usage: scripts/screenshots.sh
# Out:   build/screenshots/appstore/{en,ja}/  captioned, 2752x2064 (App Store 13-inch iPad)
#        build/screenshots/landscape/        plain screens, same size
#        docs/images/                        README images
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build/screenshots"
device_name="muxa-screenshots"
# 2064x2752: the App Store's required 13-inch iPad size.
device_type="iPad Pro 13-inch (M5)"

udid="$(xcrun simctl list devices | grep -F "$device_name (" | head -1 | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/' || true)"
if [ -z "$udid" ]; then
  udid="$(xcrun simctl create "$device_name" "$device_type")"
fi
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl status_bar "$udid" override --time "9:41" --batteryState charged --batteryLevel 100 \
  --wifiBars 3 --dataNetwork wifi

rm -rf "$out"
mkdir -p "$out/landscape" "$out/appstore"
make -C "$root" app-project >/dev/null
TEST_RUNNER_MUXA_SHOWCASE_DIR="$out/landscape" xcodebuild -project "$root/apple/Muxa/Muxa.xcodeproj" -scheme Muxa \
  -skipPackagePluginValidation -destination "platform=iOS Simulator,id=$udid" \
  -only-testing:MuxaUITests/ShowcaseTests test >"$out/xcodebuild.log" 2>&1 || {
  echo "ShowcaseTests failed; see $out/xcodebuild.log" >&2
  exit 1
}

# The App Store rejects any other size.
for shot in "$out"/landscape/*.png; do
  size="$(sips -g pixelWidth -g pixelHeight "$shot" | awk '/pixel/ { printf "%s ", $2 }')"
  if [ "$size" != "2752 2064 " ]; then
    echo "$shot is $size, not 2752x2064" >&2
    exit 1
  fi
done
swift "$root/apple/Muxa/Art/frame-screenshots.swift" "$out/landscape" "$out/appstore"

mkdir -p "$root/docs/images"
for name in 1-chat 2-agent 3-switcher 4-plan; do
  sips -Z 1600 -s format jpeg -s formatOptions 80 "$out/landscape/$name.png" \
    --out "$root/docs/images/${name#*-}.jpg" >/dev/null
done
echo "App Store: $out/appstore"
echo "README:    $root/docs/images"
