#!/usr/bin/env bash
# Takes the Google Play screenshots from the in-app showcase demo (ShowcaseTest), on a
# phone and a tablet emulator. An emulator already running that AVD is used as it is;
# one this script boots, it shuts down again.
#
# Usage: scripts/screenshots-android.sh
# Env:   MUXA_AVD_PHONE (default Pixel_10a), MUXA_AVD_TABLET (default Pixel_Tablet)
# Out:   build/screenshots/play-phone/{en,ja}/   captioned, 1080x1920
#        build/screenshots/play-tablet/{en,ja}/  captioned, 2560x1440 (the 7- and 10-inch slots)
#        build/screenshots/raw/play-{phone,tablet}/  plain screens
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/build/screenshots"
sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
adb="$sdk/platform-tools/adb"
emulator="$sdk/emulator/emulator"
studio_jdk="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
if [ -z "${JAVA_HOME:-}" ] && [ -x "$studio_jdk/bin/java" ]; then
  export JAVA_HOME="$studio_jdk"
fi
device_dir=/data/local/tmp/muxa-showcase
booted=()
serial=""

cleanup() {
  local serial
  for serial in ${booted[@]+"${booted[@]}"}; do
    "$adb" -s "$serial" emu kill >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT

# <avd>: sets `serial` to an emulator running it, booting one if none is. Not in $(...):
# the booted list must reach the cleanup.
use_avd() {
  local avd="$1" running port
  for running in $("$adb" devices | awk '/^emulator-/ { print $1 }'); do
    if [ "$("$adb" -s "$running" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$avd" ]; then
      serial="$running"
      return
    fi
  done
  port=5600
  while "$adb" devices | grep -q "^emulator-$port"; do port=$((port + 2)); done
  "$emulator" -avd "$avd" -port "$port" -no-window -no-snapshot-save -no-audio >"$out/emulator-$avd.log" 2>&1 &
  serial="emulator-$port"
  booted+=("$serial")
  "$adb" -s "$serial" wait-for-device
  until [ "$("$adb" -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
}

# <serial> enter|exit: SystemUI's demo mode, a fixed clock and full bars, as the iOS status bar override.
demo_bar() {
  local s="$1"
  if [ "$2" = exit ]; then
    "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command exit >/dev/null
    return
  fi
  "$adb" -s "$s" shell settings put global sysui_demo_allowed 1
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command enter >/dev/null
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command clock -e hhmm 0941 >/dev/null
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command battery -e level 100 -e plugged false >/dev/null
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command network -e wifi show -e level 4 >/dev/null
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command network -e mobile hide >/dev/null
  "$adb" -s "$s" shell am broadcast -a com.android.systemui.demo -e command notifications -e visible false >/dev/null
}

# <avd> <slot> <width> <height>: shoots, frames and checks one slot.
shoot() {
  local avd="$1" slot="$2" width="$3" height="$4" raw shot size
  use_avd "$avd"
  raw="$out/raw/$slot"
  rm -rf "$raw" "${out:?}/$slot"
  mkdir -p "$raw"
  "$adb" -s "$serial" shell rm -rf "$device_dir"
  demo_bar "$serial" enter
  (cd "$root/android" && ANDROID_SERIAL="$serial" ./gradlew --console=plain :app:connectedDebugAndroidTest \
    -Pandroid.testInstrumentationRunnerArguments.class=tech.xdge.muxa.app.ShowcaseTest \
    -Pandroid.testInstrumentationRunnerArguments.muxaShowcase=true) >"$out/gradle-$slot.log" 2>&1 || {
    demo_bar "$serial" exit
    echo "ShowcaseTest failed on $avd; see $out/gradle-$slot.log" >&2
    exit 1
  }
  demo_bar "$serial" exit
  "$adb" -s "$serial" pull "$device_dir/." "$raw" >/dev/null
  "$adb" -s "$serial" shell rm -rf "$device_dir"

  swift "$root/apple/Muxa/Art/frame-screenshots.swift" "$slot" "$raw" "$out/$slot"
  # Play rejects screenshots off its aspect ratios; these are the sizes its listing asks for.
  for shot in "$out/$slot"/*/*.png; do
    size="$(sips -g pixelWidth -g pixelHeight "$shot" | awk '/pixel/ { printf "%s ", $2 }')"
    if [ "$size" != "$width $height " ]; then
      echo "$shot is $size, not ${width}x$height" >&2
      exit 1
    fi
  done
}

mkdir -p "$out"
shoot "${MUXA_AVD_PHONE:-Pixel_10a}" play-phone 1080 1920
shoot "${MUXA_AVD_TABLET:-Pixel_Tablet}" play-tablet 2560 1440
echo "Google Play: $out/play-phone, $out/play-tablet"
