#!/usr/bin/env bash
# The minified release build connects over SSH (ReleaseSshTest, ADR 0029). Everything it
# reaches is throwaway: the v2 server on the fake backend (crates/muxa-server/examples/
# fake_server.rs) and an SSH host on 127.0.0.1 that `adb reverse` forwards from the device.
#
# Usage: scripts/test-android-release.sh  (make test-android-release builds the server first)
# Env:   ANDROID_SERIAL  a running emulator or device to use; the debug build's data on it is kept
#        MUXA_AVD        else boot this AVD read-only, beside any running copy (default Pixel_10a)
#        MUXA_SSH_PORT   the throwaway host's port (default 22722)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
pass() { printf 'PASS  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; exit 1; }

sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
adb="$sdk/platform-tools/adb"
emulator="$sdk/emulator/emulator"
studio_jdk="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
if [ -z "${JAVA_HOME:-}" ] && [ -x "$studio_jdk/bin/java" ]; then
  export JAVA_HOME="$studio_jdk"
fi
server="$root/target/debug/examples/fake_server"
port="${MUXA_SSH_PORT:-22722}"
avd="${MUXA_AVD:-Pixel_10a}"
# The fake server's one terminal shows this.
marker="hello"
work="$(mktemp -d "${TMPDIR:-/tmp}/muxa-release.XXXXXX")"
host_pid=""
server_pid=""
booted=""

cleanup() {
  [ -n "$host_pid" ] && kill "$host_pid" 2>/dev/null || true
  [ -n "$server_pid" ] && kill "$server_pid" 2>/dev/null || true
  if [ -n "${ANDROID_SERIAL:-}" ]; then
    "$adb" reverse --remove "tcp:$port" >/dev/null 2>&1 || true
  fi
  [ -n "$booted" ] && "$adb" -s "$booted" emu kill >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

[ -x "$server" ] || { echo "build the server first: cargo build -p muxa-server --example fake_server" >&2; exit 1; }

"$server" "$work/server.sock" >"$work/server.log" 2>&1 &
server_pid=$!
for _ in $(seq 100); do
  grep -q '^ready' "$work/server.log" && break
  sleep 0.1
done
grep -q '^ready' "$work/server.log" || { echo "the v2 server did not start" >&2; exit 1; }

(cd "$root/android" && ./gradlew --console=plain -q :testhost:installDist)
# Every exec reaches the server's socket, as `muxa-bridge connect` does.
"$root/android/testhost/build/install/testhost/bin/testhost" "$port" nc -U "$work/server.sock" \
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
  grep -q 'name="connectsOverSshAndShowsATerminal"' "$report" && passed=1
done
[ -n "$passed" ] || fail "the release test did not run"
pass "the release build connected over SSH and showed a terminal"
