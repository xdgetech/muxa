# 0030: Full R8 for Android release builds, checked by a release-build SSH test

## Status

Proposed. Replaces the R8 decision of ADR 0014 ("R8 only removes code"). The rest of ADR 0014 stands.

## Context

- ADR 0014 turned R8 on only to drop BouncyCastle's EST package, whose trust-everything `X509TrustManager` Play's security scan rejects. It kept all of muxa, sshj, JediTerm, slf4j and BouncyCastle, and it renamed and optimized nothing. The release APK was about 16.5 MB.
- No test ran the minified app. A wrong keep rule shows up only on a real connection, so full R8 needed a release-build test that connects over SSH (`docs/plan.md`).
- What is created from a class name, checked in the jars:
  - BouncyCastle: `BouncyCastleProvider` loads each algorithm's `$Mappings` class by name, and the JCA creates each SPI class by name (`org.bouncycastle.jcajce.provider`, `org.bouncycastle.pqc.jcajce.provider`, `org.bouncycastle.jce.provider`).
  - sshj: only `Class.forName("org.bouncycastle.jce.provider.BouncyCastleProvider")`. The app registers that provider itself.
  - JediTerm: nothing.
  - slf4j finds its provider with `ServiceLoader`; R8 rewrites `META-INF/services` itself.
  - kotlinx.serialization, coroutines, Compose and androidx.webkit ship their own rules. The default `proguard-android-optimize.txt` keeps `@JavascriptInterface` methods.

## Decision

- **R8 removes, renames and optimizes the release build.** `android/app/proguard-rules.pro` keeps only BouncyCastle's provider classes, with their names and constructors. Nothing keeps EST, so R8 still drops it.
- **`make test-android-release` is the check** (`scripts/test-android-release.sh`, `ReleaseSshTest`).
  1. It starts a tmux server of its own (`-L muxa-test-release-*`) with a pane that prints a marker.
  2. It starts a throwaway SSH host (`android/testhost`, Apache MINA SSHD) on 127.0.0.1. The host has a new host key each run, accepts any client key and has no shell or password login. Every exec runs the built `muxa-bridge` on that tmux server, with its own state, daemon and agent dirs, as a `command=` entry in `authorized_keys` does.
  3. `adb reverse` forwards the device's port to it. The script boots a read-only emulator unless `ANDROID_SERIAL` names a device.
  4. The test runs against the minified release build (`testBuildType = "release"` under `-Pmuxa.releaseTest=true`). It adds the host in the app, trusts the host key, and waits for the marker in the pane.
- **The test drives the app with UI Automator**, through the accessibility tree. It names no app class or member, so R8's renaming cannot break the test itself.
- **The test build differs from the shipped build in two ways only:**
  - It is signed with the debug key, never the upload key. `bundleRelease` refuses to run with `muxa.releaseTest`.
  - The test APK calls the app's copies of `androidx.tracing` and the Kotlin standard library, so `proguard-release-test.pro` keeps them. Nothing in it touches muxa, sshj, JediTerm or BouncyCastle.

## Consequences

- The release APK is 8.3 MB instead of 16.5 MB (unsigned, `assembleRelease`).
- A change to the keep rules, a dependency or the SSH path needs `make test-android-release` before an upload (`store/RELEASE.md`). It needs an emulator and takes a few minutes, so it is not part of `make check`.
- Checked: with the three BouncyCastle rules removed, the test fails ("the pane never showed …"). With them, it passes.
- Gradle reports success when the app crashes before any test runs. The script reads the test report itself.
- Try the Demo in the release build is still checked by hand, as in ADR 0014.
