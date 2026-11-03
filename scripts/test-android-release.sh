#!/usr/bin/env bash
# The minified release build connects over SSH (ReleaseSshTest, ADR 0029). Everything it
# reaches is throwaway: a tmux server of its own, a bridge with its own state dirs, and an
# SSH host on 127.0.0.1 that `adb reverse` forwards from the device.
#
# Usage: scripts/test-android-release.sh  (make test-android-release builds the bridge first)
# Env:   ANDROID_SERIAL  a running emulator or device to use; the debug build's data on it is kept
#        MUXA_AVD        else boot this AVD read-only, beside any running copy (default Pixel_10a)
#        MUXA_SSH_PORT   the throwaway host's port (default 22722)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
MUXA_SOCKET="muxa-test-release-$$"
. "$root/scripts/lib/tmux.bash"

sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
adb="$sdk/platform-tools/adb"
emulator="$sdk/emulator/emulator"
studio_jdk="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
if [ -z "${JAVA_HOME:-}" ] && [ -x "$studio_jdk/bin/java" ]; then
  export JAVA_HOME="$studio_jdk"
fi
bridge="$root/target/debug/muxa-bridge"
port="${MUXA_SSH_PORT:-22722}"
avd="${MUXA_AVD:-Pixel_10a}"
marker="muxa-release-$$"
work="$(mktemp -d "${TMPDIR:-/tmp}/muxa-release.XXXXXX")"
host_pid=""
booted=""

cleanup() {
  [ -n "$host_pid" ] && kill "$host_pid" 2>/dev/null || true
  if [ -n "${ANDROID_SERIAL:-}" ]; then
    "$adb" reverse --remove "tcp:$port" >/dev/null 2>&1 || true
  fi
  [ -n "$booted" ] && "$adb" -s "$booted" emu kill >/dev/null 2>&1 || true
  mt_down
  rm -rf "$work"
}
trap cleanup EXIT

[ -x "$bridge" ] || { echo "build the bridge first: cargo build -p muxa-bridge" >&2; exit 1; }

# The pane the app must show.
mt new-session -d -s release -x 80 -y 24 "printf '%s\\n' '$marker'; exec cat"

(cd "$root/android" && ./gradlew --console=plain -q :testhost:installDist)
# The bridge's own state, daemon and agent dirs, so it never reads the developer's.
mkdir -p "$work/state" "$work/daemon" "$work/agents"
"$root/android/testhost/build/install/testhost/bin/testhost" "$port" "$bridge" \
  --socket "$MUXA_SOCKET" --idle-timeout 300 --state-dir "$work/state" --daemon-dir "$work/daemon" \
  --workspaces "$work/workspaces.toml" --agent-dir "claude-code=$work/agents" --agent-dir "codex=$work/agents" \
  >"$work/testhost.log" 2>&1 &
host_pid=$!
for _ in $(seq 100); do
  grep -q '^listening' "$work/testhost.log" && break
  kill -0 "$host_pid" 2>/dev/null || { cat "$work/testhost.log" >&2; exit 1; }
  sleep 0.2
done
grep -q '^listening' "$work/testhost.log" || { echo "the SSH host did not start" >&2; exit 1; }

if [ -z "${ANDROID_SERIAL:-}" ]; then
  # A read-only instance runs beside any other copy of the AVD and keeps nothing.
  emu_port=5700
  while "$adb" devices | grep -q "^emulator-$emu_port"; do emu_port=$((emu_port + 2)); done
  "$emulator" -avd "$avd" -port "$emu_port" -read-only -no-window -no-snapshot -no-audio >"$work/emulator.log" 2>&1 &
  booted="emulator-$emu_port"
  export ANDROID_SERIAL="$booted"
  "$adb" wait-for-device
  until [ "$("$adb" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
  "$adb" uninstall tech.xdge.muxa >/dev/null 2>&1 || true
fi
"$adb" reverse "tcp:$port" "tcp:$port" >/dev/null

results="$root/android/app/build/outputs/androidTest-results/connected/release"
rm -rf "$results"
cd "$root/android"
./gradlew --console=plain -Pmuxa.releaseTest=true :app:connectedReleaseAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=tech.xdge.muxa.app.ReleaseSshTest \
  -Pandroid.testInstrumentationRunnerArguments.muxaSshPort="$port" \
  -Pandroid.testInstrumentationRunnerArguments.muxaMarker="$marker"

# Gradle succeeds even when the app crashes before any test runs, so read the report.
passed=""
for report in "$results"/TEST-*.xml; do
  [ -f "$report" ] || continue
  if grep -q '<failure' "$report"; then
    cat "$report" >&2
    fail "the release test failed (logcat has the crash)"
  fi
  grep -q 'name="connectsOverSshAndShowsThePane"' "$report" && passed=1
done
[ -n "$passed" ] || fail "the release test did not run"
pass "the release build connected over SSH and showed the pane"
