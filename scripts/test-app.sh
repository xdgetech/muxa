#!/usr/bin/env bash
# Called under scripts/ios-simulator.py's shared lock.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
: "${IOS_QA_UDID:?run through make test-app or mise run qa:ios:iphone}"
: "${IOS_QA_DERIVED_DATA:?missing QA storage}"
: "${IOS_QA_OUTPUT_ROOT:?missing QA output directory}"
make -C "$root" app-project
export TEST_RUNNER_MUXA_SCREENSHOT_DIR="${TEST_RUNNER_MUXA_SCREENSHOT_DIR:-$IOS_QA_OUTPUT_ROOT/screenshots}"
exec xcodebuild -project "$root/apple/Muxa/Muxa.xcodeproj" -scheme Muxa \
  -skipPackagePluginValidation -destination "platform=iOS Simulator,id=$IOS_QA_UDID" \
  -derivedDataPath "$IOS_QA_DERIVED_DATA" -resultBundlePath "$IOS_QA_OUTPUT_ROOT/tests.xcresult" \
  -parallel-testing-enabled NO test
