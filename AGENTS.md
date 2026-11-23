# AGENTS.md

Instructions for coding agents working in this repo. Humans: see `README.md` and `CONTRIBUTING.md`.

muxa lets you run and follow coding agents from an iPad, an iPhone, an Android phone, tablet or foldable, or a Mac: an Inbox in your own order, Tasks with their own places, Routines, Task Loops, the Reader and Chat. A daemon on the host serves protocol v2 and runs every agent as a herdr-core terminal (ADR 0067). Native clients show a terminal at its real size. Status: pre-alpha. The status and roadmap are in `docs/plan.md`.

## Read first

1. `docs/plan.md`: status, what comes next, the design in short, and how each part is verified.
2. `protocol/PROTOCOL-v2.md`: the wire format. Clients and the daemon follow it exactly.
3. `docs/findings/`: verified behavior of herdr-core and the agents (`session-backends.md`, `claude-code-*.md`). Do not re-derive it; extend it.
4. `docs/adr/`: decisions and why. Do not reverse one silently. Propose a new ADR instead.
5. `docs/vision.md`: where muxa is heading. Only `docs/plan.md` Next is a commitment.

## Map

| Path | What |
|---|---|
| `crates/muxa-backend` | the session backend trait (ADR 0067): one terminal at a time, no Workspaces or layout. Its `contract` feature is the suite every adapter must pass; its `fake` feature is the in-memory backend tests use. |
| `crates/muxa-herdr` | herdr-core client (JSON-RPC calls, attach streams with credit, events) and `HerdrBackend`, the backend over it. The only code that talks to herdr-core. |
| `crates/muxa-inbox` | the Inbox of ADR 0066: items in the user's order, Later, Done, the Capture parser, and `<state dir>/inbox.json`. No i/o but its own file. |
| `crates/muxa-server` | the client sessions of protocol v2: attach, input with acks, credit, resume (attachments parked with a 256 KiB buffer), Task start (`src/tasks.rs`: the host prepares the places, the server runs the agent with `muxa.*` labels), a Task's shells (`src/shells.rs`, ADR 0079), `watch`, the Reader (`src/reader.rs`), Task Loops, Chat and Settings through the `Host` trait; `src/memory_settings.rs` keeps Settings in memory for the fake server and tests. Generic over the session backend; it never touches files. |
| `crates/muxa-protocol` | protocol v2's frames and messages (`src/v2.rs`), and the data types the daemon shares with them: agent conversations, the Reader's entries, Routine state. |
| `crates/muxa-bridge` | the host binary. `muxa-bridge connect` relays a client to the daemon's `server.sock`; `muxa-bridge daemon …` runs and installs the daemon (`src/daemon/`). `src/v2/` is the daemon's `Host`: `tasks.rs` and `places.rs` make a Task's places (a folder, branch worktrees, read-only repos; ADR 0068) and pick its agent, `runner.rs` runs Routines, `task_loop.rs` runs Task Loops, `reader.rs` serves the Reader, `chat.rs` follows an agent's conversation, and `feed.rs` copies Routine output into the Inbox. `src/agents/` holds the agent adapters (`docs/agents.md`), `src/connections.rs` the connections from the Keychain and `src/v2/mcp.rs` the MCP servers an agent gets (ADR 0069), `src/routines/` the Routine schedule, store, check and publish (ADR 0026), `src/records/` the `workspaces.toml` config, and `src/settings/` the Settings the apps edit (ADR 0074): the view, the ops on the files with `toml_edit`, atomic writes, and Keychain writes through `security -i`. |
| `crates/muxa-cli` | the terminal reference client and end-to-end harness: every protocol v2 request, with resume, as the apps do it. `muxa-cli v2 …` still works. |
| `apple/MuxaKit` | Swift package: `MuxaTransport` (SSH exec, the local socket, keys), `MuxaSession` (hosts, keys, the pane grid), the UIKit pane views (`MuxaUI`) and `MuxaV2`, the protocol v2 client (`V2Client` with resume) over a `ByteChannel`, with `DemoChannel`, the in-memory server of Try the Demo. |
| `apple/Muxa` | the iPad and iPhone app (SwiftUI). The layout follows size classes, not the device (ADR 0012). `Muxa/Next/` holds the screens both Apple apps share around `NextModel`: the Inbox, Capture, Routines, Needs you, Chat, the Reader and the Task Loop. `Muxa/Next/Touch/` holds the iOS layouts of the design mocks: on a phone the Inbox, Workspaces, Routines and Settings tabs and a Task screen with Chat, Terminal and Reader segments; on a tablet a sidebar (Inbox, Tasks, Routines) beside the picked item, Task or Routine. `Muxa/Theme.swift` holds the mocks' dark palette and the kind and state colors. `Muxa/Reader/` holds the Reader's web views and assets, which Android and the Mac use too (vendored JS: `scripts/vendor-reader.sh`). `Muxa/Fonts/` holds the terminal's bundled fonts (`scripts/vendor-fonts.sh`, ADR 0080); `Next/TerminalSettings.swift` the device's terminal colors and font. `project.yml` generates the Xcode project. **Try the Demo** (every build) runs `MuxaV2.DemoChannel`; the UI tests (`NextTests`) launch with `-demo`. |
| `apple/Muxa/MuxaMac` | the Mac app (ADR 0070): a native macOS target that compiles `Muxa/Next/` (not `Touch/`) and `Muxa/Reader/` (`Next/Platform.swift` holds the iOS and Mac differences) with its own Home and Focus (`MuxaMac/HomeView.swift`, `FocusView.swift`), its own `TerminalPane` on SwiftTerm's AppKit `TerminalView`, the Go, Pane and Host menus, multi-pane Focus (`MuxaMac/PaneSplit.swift`), the user's keys and Vim's window keys (`MuxaMac/KeyBindings.swift`, `WindowKeyMonitor.swift`; ADR 0078), the `⌃⌥Space` Capture window (`MuxaMac/QuickCapture.swift`), and "This Mac", which reaches the daemon's `server.sock` through `MuxaTransport.UnixSocketBytes`. `MuxaMacUITests` run it with `-demo`. |
| `android/` | the Android client (Kotlin, Gradle; ADR 0013). `:terminal` (the pane screen on JediTerm), `:transport` (sshj; `ByteChannel` with `SshExec` and `ProcessBytes`), `:v2` (the protocol v2 client, `V2Client` with resume; `DemoChannel`, the in-memory server of Try the Demo; `KeyBytes`) and `:session` (the hosts, keys and pane fit) are plain JVM modules with JVM tests; `:app` is the Compose UI: `app/next/` holds Home and Focus around `NextModel`, one per window (ADR 0029); `:testhost` is the throwaway SSH host of the release-build test. Debug builds start Try the Demo with `adb shell am start -n tech.xdge.muxa/tech.xdge.muxa.app.MainActivity --ez demo true`. |
| `store/` | the store listings (fastlane `deliver` and `supply` layout, English and Japanese), Play's icon and feature graphic, and `RELEASE.md`: every console answer, the price and the release steps. `docs/privacy.md` is the privacy policy. |
| `scripts/` | harness: the herdr-core repro script, the iOS simulator lock, the release test, the store text check, the agent guard hook. |
| `tests/harness/` | bats tests for the harness itself. |

## Commands

| Command | Use |
|---|---|
| `make check` | fmt, clippy, unit tests, shellcheck, harness tests. Must be green before every commit. |
| `make install` | **humans only**: installs `muxa-bridge` and `muxa-cli` into `~/.local/bin` (`PREFIX` picks another root) and restarts a running daemon. It replaces the binaries the developer's devices use. |
| `make test-herdr` | **humans only**: the herdr-core client, the backend contract, the v2 server and the daemon's v2 socket against the running herdr-core. It adds one labelled terminal per test and removes it. |
| `scripts/repro/herdr-core-api.sh` | re-checks the herdr-core facts in `docs/findings/session-backends.md` on a throwaway herdr-core. |
| `make test-apple` | MuxaKit tests (macOS, Xcode toolchain): v2 frames and the demo server, transport and key setup, hosts, keys and the grid. The Makefile uses `/Applications/Xcode.app` when xcode-select points to the Command Line Tools; set `DEVELOPER_DIR` to use another Xcode. |
| `make test-apple-v2` | the `MuxaV2` tests, also against the real server on the fake backend (`crates/muxa-server/examples/fake_server.rs`). |
| `make test-android` | the Android client's JVM tests: the terminal, transport, session (hosts, keys, fit) and `:v2`. Uses Android Studio's JDK when `JAVA_HOME` is unset. |
| `make test-android-v2` | the Android `:v2` tests, also against the real server on the fake backend. |
| `make build-app` / `make test-app` | generate the Xcode project (XcodeGen) and build the app, or run its UI tests on an iPad against the in-app demo server (`MUXA_SIM` picks the simulator). |
| `make test-app-iphone` | the same UI tests on an iPhone (`MUXA_SIM_IPHONE`). |
| `make build-mac-app` / `make test-mac-app` | build the Mac app signed to run locally, or run its UI tests on the in-app demo server. macOS asks once to allow UI automation (`automationmodetool`). |
| `make install-mac` | **humans only**: build the Mac app, replace `/Applications/Muxa.app` (`MAC_APPS` picks another folder) and open it. A running Muxa is quit first. `MAC_SIGNED=1` signs with the team in `Local.xcconfig` and creates the Mac's provisioning profile, so SSH hosts work; run it on the Mac that will use the app. |
| `make test-android-release` | the minified release build (full R8, ADR 0030) connects over SSH to a throwaway host on 127.0.0.1, which runs the v2 server on the fake backend, and shows a terminal in Focus. Boots a read-only `Pixel_10a` emulator (`MUXA_AVD`) unless `ANDROID_SERIAL` names one. |
| `make build-android` / `make lint-android` | build the debug app, or run Android lint on it. |
| `make test-android-app` | the app's UI tests (`NextTest` against the in-app demo server, and the terminal view's input) on the running emulator or device. |
| `make install-iphone` / `make install-ipad` | **humans only**: build, sign, install and open the app on a paired iPhone or iPad. The device's name or identifier comes from `MUXA_IPHONE` or `MUXA_IPAD` (`MUXA_DEVICE` still works for the iPad); list them with `xcrun devicectl list devices`. A locked device gets the app; unlock it to open Muxa. It creates signing assets on the developer's Apple account. |
| `make app-icon` | renders the icon from `apple/Muxa/Art/app-icon.swift`: iOS's light, dark and tinted images, the Mac's sizes, Android's adaptive icon layers, and Play's store icon and feature graphic. |
| `make lint-store` | the store listings fit each store's limits and never say "demo" or "trial" (part of `make check`). |
| `make bundle-android` | the signed Play bundle into `build/release/muxa.aab`, with the upload key from `~/.gradle/gradle.properties` (`store/RELEASE.md`). |
| `make archive-ios` | **humans only**: a signed release archive and the `.ipa` for App Store Connect, in `build/release/`. It signs with the developer's Apple account. |
| `muxa-cli --bridge 'ssh host ~/.local/bin/muxa-bridge connect' terminals` | the reference client against a host's daemon. |

## Invariants

Breaking one of these breaks the product promise. Each has a reason in `docs/findings/` or `docs/adr/`.

1. **Never touch the developer's running agents.** Tests run on the fake backend. Only `make test-herdr`, which humans run, reaches the developer's herdr-core, and it adds and removes only its own labelled terminals. A Claude Code hook (`scripts/hooks/guard-tmux.py`) still blocks `tmux` on the default server, where older agents may run. Do not work around it.
2. **muxa finds its own terminals by their `muxa.*` labels** (`muxa.kind`, `muxa.dir`, `muxa.workspace`…) and targets them by id. It never changes a terminal it did not start, except by the user's own input.
3. **Untrusted input stays data.** A client names a Reader file only by a path from the list the daemon made, and the file must stay inside its folder. A check command must be one the Workspace's `[workspace.loop] checks` allows. A client may edit `workspaces.toml` and `routines.toml` only through `settings-update` ops, which the daemon applies after the loading rules accept the result and the revision still matches, unless the host runs with `--settings-read-only` (ADR 0074); env values and secrets go in and never come back. Branch and base names go through `git check-ref-format`. Agent records are parsed, never executed.
4. **Terminal bytes, agent transcripts, documents, diffs and check output never reach logs.** Log ids, paths, sizes and errors.
5. **Secrets come from the Keychain and go only into an agent's environment** (ADR 0069). They never reach a log, an error, a file in the repo or the wire back to a client. A client may send one in, through `settings-update`, and it goes only to the Keychain (ADR 0074).
6. **No invented agent state.** Report the state the program reports itself (OSC 7501), or none.
7. **muxa never answers an agent's menu by itself.** The approval card offers only keys an adapter checked against that agent's menu (ADR 0009), and only the user picks one.

## Definition of done

- `make check` is green. When the change touches the clients, the matching `make test-apple`, `make test-apple-v2`, `make test-android` and `make test-android-v2` are green too.
- New herdr-core or agent behavior you rely on has a row in `docs/findings/` and, where it can be checked, a step in `scripts/repro/`.
- A decision that a reviewer could reasonably make differently gets an ADR in `docs/adr/`.
- `docs/plan.md` still describes reality. Update it when a feature ships or the design changes.
- A change to the wire format updates `protocol/PROTOCOL-v2.md` and the Rust, Swift and Kotlin types together, and the demo servers when they answer it.

## Conventions

- Rust:
  - Edition 2024, stable toolchain.
  - `thiserror` for library errors, with context on each error.
  - No `unwrap`/`expect` outside tests and provably impossible cases.
  - No `unsafe`.
- Shell:
  - Bash 3.2 compatible, because macOS ships it. Example: use `${arr[@]+"${arr[@]}"}` for arrays that may be empty under `set -u`.
  - shellcheck clean.
- Comments explain why, not what. Keep them short.
- Tests check observable behavior: bytes on the wire, screen contents, files written. Never delete or loosen a test to make it pass.
- No personal host names, user paths or private setup anywhere in the repo.

## Git

- For now, commits go straight to `main` after `make check`. Commits use `<type>: <description>` (`feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`). The subject is at most 72 characters.
- When pull requests are used again, open them as drafts. A human marks them ready.
