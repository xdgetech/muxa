#!/usr/bin/env bash
# The app's UI tests against the real bridge on a throwaway tmux server (RealTmuxTests).
# Called under scripts/ios-simulator.py's shared lock: make test-app-tmux.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
: "${IOS_QA_UDID:?run through make test-app-tmux}"
: "${IOS_QA_DERIVED_DATA:?missing QA storage}"
: "${IOS_QA_OUTPUT_ROOT:?missing QA output directory}"
export MUXA_SOCKET="muxa-test-ui-$$"
# shellcheck source=lib/tmux.bash
. "$root/scripts/lib/tmux.bash"

state="$(mktemp -d "${TMPDIR:-/tmp}/muxa-ui-tmux.XXXXXX")"
relay=""
cleanup() {
  [ -z "$relay" ] || kill "$relay" 2>/dev/null || true
  mt_down
  rm -rf "$state"
}
trap cleanup EXIT

cargo build -q --manifest-path "$root/Cargo.toml" -p muxa-bridge
mt new-session -d -s shell -x 80 -y 24 sh
mt set-option -g default-shell /bin/sh
# 500 numbered lines, most of them in the pane's scrollback.
mt send-keys -t shell -l "seq -f 'row %03g' 1 500"
mt send-keys -t shell Enter
until [ "$(mt display-message -p -t shell '#{history_size}')" -ge 400 ]; do sleep 0.1; done
# An agent that clears tmux's history when its window is resized, as the device does.
mt new-session -d -s agent -x 80 -y 24 "$root/scripts/lib/clearing-agent.sh"
until [ "$(mt display-message -p -t agent '#{history_size}')" -ge 400 ]; do sleep 0.1; done

env -u TMUX XDG_STATE_HOME="$state/state" XDG_CONFIG_HOME="$state/config" \
  python3 "$root/scripts/lib/bridge-relay.py" -- "$root/target/debug/muxa-bridge" --socket "$MUXA_SOCKET" \
  >"$state/port" &
relay=$!
until [ -s "$state/port" ]; do sleep 0.1; done
export TEST_RUNNER_MUXA_BRIDGE_PORT
TEST_RUNNER_MUXA_BRIDGE_PORT="$(cat "$state/port")"

make -C "$root" app-project
export TEST_RUNNER_MUXA_SCREENSHOT_DIR="${TEST_RUNNER_MUXA_SCREENSHOT_DIR:-$IOS_QA_OUTPUT_ROOT/screenshots}"
mkdir -p "$TEST_RUNNER_MUXA_SCREENSHOT_DIR"
xcodebuild -project "$root/apple/Muxa/Muxa.xcodeproj" -scheme Muxa \
  -skipPackagePluginValidation -destination "platform=iOS Simulator,id=$IOS_QA_UDID" \
  -derivedDataPath "$IOS_QA_DERIVED_DATA" -resultBundlePath "$IOS_QA_OUTPUT_ROOT/tests.xcresult" \
  -parallel-testing-enabled NO -only-testing:MuxaUITests/RealTmuxTests test
