#!/usr/bin/env bash
# Takes the App Store and README screenshots from the in-app showcase demo
# (ShowcaseTests), on the same host-configured devices as other QA.
#
# Usage: scripts/screenshots.sh
# Out: a fresh run directory on the configured QA volume; previous runs and
# docs/images are preserved. The printed path contains App Store and README images.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
if [ "${1:-}" != --locked ]; then
  exec python3 "$root/scripts/ios-simulator.py" run ipad-pro iphone -- bash "$0" --locked
fi
: "${IOS_QA_OUTPUT_ROOT:?missing QA output directory}"
: "${IOS_QA_DERIVED_DATA:?missing QA storage}"
out="$IOS_QA_OUTPUT_ROOT/screenshots"
active_udid=""
started=0

restore() {
  [ -n "$active_udid" ] || return 0
  xcrun simctl status_bar "$active_udid" clear >/dev/null 2>&1 || true
  if [ "$started" = 1 ]; then
    xcrun simctl shutdown "$active_udid" >/dev/null 2>&1 || true
  fi
  active_udid=""
}
trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# <existing UDID> <slot> <width> <height>: shoots, frames and checks one slot.
shoot() {
  local udid="$1" slot="$2" width="$3" height="$4" raw shot size booted
  active_udid="$udid"
  started=0
  booted="$(xcrun simctl list devices booted)"
  if ! printf '%s\n' "$booted" | grep -Fq "$udid"; then
    xcrun simctl boot "$udid"
    started=1
  fi
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl status_bar "$udid" override --time "9:41" --batteryState charged --batteryLevel 100 \
    --wifiBars 3 --dataNetwork wifi

  raw="$out/raw/$slot"
  mkdir -p "$raw"
  TEST_RUNNER_MUXA_SHOWCASE_DIR="$raw" xcodebuild -project "$root/apple/Muxa/Muxa.xcodeproj" -scheme Muxa \
    -skipPackagePluginValidation -destination "platform=iOS Simulator,id=$udid" \
    -derivedDataPath "$IOS_QA_DERIVED_DATA" -resultBundlePath "$out/$slot.xcresult" \
    -parallel-testing-enabled NO \
    -only-testing:MuxaUITests/ShowcaseTests test >"$out/xcodebuild-$slot.log" 2>&1 || {
    echo "ShowcaseTests failed for $slot; see $out/xcodebuild-$slot.log" >&2
    exit 1
  }
  restore

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
shoot "${IOS_QA_IPAD_PRO_UDID:?missing shared iPad}" ipad 2752 2064
shoot "${IOS_QA_IPHONE_UDID:?missing shared iPhone}" iphone 1320 2868

mkdir -p "$out/readme"
for name in 1-chat 2-agent 3-switcher 4-plan; do
  sips -Z 1600 -s format jpeg -s formatOptions 80 "$out/raw/ipad/$name.png" \
    --out "$out/readme/${name#*-}.jpg" >/dev/null
done
echo "App Store: $out/appstore-ipad, $out/appstore-iphone"
echo "README:    $out/readme (copy selected images into docs/images when ready)"
