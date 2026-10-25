# AGENTS.md

Instructions for coding agents working in this repo. Humans: see `README.md` and `CONTRIBUTING.md`.

muxa lets you work on your **existing** tmux panes from an iPad, without changing the desktop layout. A Rust bridge runs on the host over SSH and speaks tmux control mode. Native clients render one pane at its real size. Status: pre-alpha. The roadmap and current phase are in `docs/plan.md`.

## Read first

1. `docs/plan.md`: phases, what is in scope now, the definition of done per phase.
2. `docs/findings/tmux-control-mode.md`: verified tmux behavior. Do not re-derive it; extend it.
3. `docs/adr/`: decisions and why. Do not reverse one silently. Propose a new ADR instead.
4. `docs/product-brief.md`: the original product brief.

## Map

| Path | What |
|---|---|
| `crates/muxa-tmux` | control-mode client, quoting, discovery, snapshot, input. The only code that talks to tmux. |
| `crates/muxa-protocol` | wire types and version. Must match `protocol/PROTOCOL.md` and `protocol/fixtures`. |
| `crates/muxa-bridge` | the bridge binary: protocol over stdin/stdout. `src/agents/` holds the agent adapters behind `chat` (`docs/agents.md`); `src/docs/` serves the Reader's documents. |
| `crates/muxa-cli` | terminal reference client and end-to-end harness. |
| `apple/MuxaKit` | Swift package: protocol, transport (SSH), `PaneSession`, and the UIKit pane views (`MuxaUI`). |
| `apple/Muxa` | the iPad app (SwiftUI). `Muxa/Reader/` renders documents offline (vendored JS: `scripts/vendor-reader.sh`). `project.yml` generates the Xcode project; launch with `-demo` for a bridge-free demo (DEBUG). |
| `scripts/` | harness: sandbox, layout snapshot, repro scripts, the agent guard hook. |
| `tests/harness/` | bats tests for the harness itself. |

## Commands

| Command | Use |
|---|---|
| `make check` | fmt, clippy, unit tests, shellcheck, harness tests. Must be green before every commit. |
| `make test-tmux` | integration tests on throwaway tmux servers. |
| `make repro` | re-checks every tmux fact in `docs/findings/`. |
| `make test-apple` | MuxaKit tests (macOS, Xcode toolchain): protocol fixtures, SwiftTerm scenario replay, the pane session. The Makefile uses `/Applications/Xcode.app` when xcode-select points to the Command Line Tools; set `DEVELOPER_DIR` to use another Xcode. |
| `make test-apple-tmux` | the same, plus `PaneSession` against the real bridge on a throwaway tmux server. |
| `make build-app` / `make test-app` | generate the Xcode project (XcodeGen) and build the iPad app, or run its UI test against the in-app demo bridge (`MUXA_SIM` picks the simulator). |
| `MUXA_DEVICE=<name or id> make install-ipad` | **humans only**: build, sign and run on a paired iPad. It creates signing assets on the developer's Apple account. |
| `TEST_RUNNER_MUXA_SCREENSHOT_DIR=<dir> make test-app` | also keeps the UI test's portrait and landscape screenshots. |
| `MUXA_STREAM_RUNS=200 make test-tmux` | more random cut points for the snapshot/stream stress test (default 50). |
| `MUXA_TEST_PLUGIN=<plugin.tmux> make test-tmux` | also checks that a real layout plugin keeps its layout. |
| `MUXA_RECORD_SCENARIOS=1 cargo test -p muxa-bridge --features tmux-integration --test stream` | re-records `protocol/fixtures/scenarios/`. |
| `make sandbox` / `make sandbox-down` | an isolated tmux server (`-L muxa-sandbox`) with two agent + shell sessions. |
| `scripts/layout-snapshot.sh record -L <socket>` | record a layout; `diff before after` proves nothing changed. |
| `muxa-cli --bridge 'muxa-bridge --socket muxa-sandbox' watch demo-a` | the reference client against the sandbox (Ctrl-] leaves). |
| `MUXA_LIVE=1 scripts/live-check.sh SESSION` | **humans only**: the before/after layout proof on the real server. Agents never set `MUXA_LIVE`. |

## Invariants

Breaking one of these breaks the product promise. Each has a reason in `docs/findings/` or `docs/adr/`.

1. **Never touch the default tmux server.** The developer's live agents run there. Use `tmux -L muxa-test-<name>` or the sandbox. A Claude Code hook (`scripts/hooks/guard-tmux.py`) blocks `tmux` without `-L`/`-S`. Do not work around it. If a live read is needed, ask the user to run it with `!`.
2. **Never change the size or layout of a window or pane.** The bridge attaches with `-f ignore-size`. It never sends `refresh-client -C`, `resize-pane`, `resize-window`, `select-window`, `select-pane`, or `refresh-client -A '%p:off'`.
3. **Target tmux objects by id** (`$n`, `@n`, `%n`), never by name. Names resolve ambiguously.
4. **Untrusted input stays data.** Pane ids are validated. Every value placed in a tmux command line goes through the quoting function in `muxa-tmux`. No other path builds command lines.
5. **Pane bytes, agent transcripts and documents never reach logs.** Log ids, paths, sizes and errors, not terminal, conversation or document content.
6. **No invented agent state.** Report the state an adapter supplies, with its age. Otherwise report `unknown`.
7. **Read tmux output as bytes.** `%output` carries raw non-UTF-8 bytes.

## Definition of done

- `make check` is green. When the change touches tmux behavior, `make test-tmux` and `make repro` are green too.
- New tmux behavior you rely on has a script in `scripts/repro/` and a row in `docs/findings/tmux-control-mode.md`.
- A decision that a reviewer could reasonably make differently gets an ADR in `docs/adr/`.
- `docs/plan.md` still describes reality. Update it when a phase finishes or the design changes.
- A change to the wire format updates `protocol/PROTOCOL.md`, the fixtures and the Rust and Swift types together, and bumps the version when the change is incompatible.

## Conventions

- Rust:
  - Edition 2024, stable toolchain.
  - `thiserror` for library errors, with context on each error.
  - No `unwrap`/`expect` outside tests and provably impossible cases.
  - No `unsafe`.
- Shell:
  - Bash 3.2 compatible, because macOS ships it. Example: use `${arr[@]+"${arr[@]}"}` for arrays that may be empty under `set -u`.
  - shellcheck clean.
  - Scripts source `scripts/lib/tmux.bash` and use `mt`, which always passes `-L`.
- Comments explain why, not what. Keep them short.
- Tests check observable behavior: bytes on the wire, screen contents, pane sizes. Never delete or loosen a test to make it pass.
- No personal host names, user paths or private setup anywhere in the repo.

## Git

- For now, commits go straight to `main` after `make check`. Commits use `<type>: <description>` (`feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`). The subject is at most 72 characters.
- When pull requests are used again, open them as drafts. A human marks them ready.
