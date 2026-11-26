# Contributing

Thanks for helping. muxa runs other people's agents on their own machine; most of the process below keeps it from touching what it did not start, and keeps their content out of logs.

## Setup

- Rust stable (`rustup`; `rust-toolchain.toml` selects the components).
- bats, shellcheck, jq, python3.
- For the apps: Xcode 16+, Android Studio.
- To try the daemon for real: herdr-core.

```sh
make check   # must be green before you open a pull request
```

If your `shellcheck` lives elsewhere: `make check SHELLCHECK="<command>"`.

## Working with herdr-core

- Tests run on the fake backend (`muxa-backend` with `fake`). Never point a test at your own herdr-core; `make test-herdr` is for humans, and it adds and removes only its own labelled terminals.
- If you rely on a herdr-core or agent behavior, write it down in `docs/findings/` and check it in `scripts/repro/` where you can.

## Disk space

A Task worktree builds its own `target/` (several GB each). muxa can put that output on another disk (ADR 0102):

```toml
# workspaces.toml
[build]
dir = "/Volumes/External/muxa/build"

[[workspace]]
# …
[workspace.build]
paths = ["target", "apple/MuxaKit/.build"]
```

Settings in the apps edits `[build]` and each Workspace's build paths, and says when the host cannot find the folder, for example while the disk is not mounted.

`muxa-cli disk` shows what the Task places, other worktrees and build folders use; `muxa-cli disk clean` says what it would remove, and `--yes` removes it. `cargo clean` in a worktree replaces the `target` link with a real folder; start the Task again, or remove the folder, to get the link back.

muxa does not manage tool caches. They can be large on a machine that runs the app tests:
- iOS simulators (`~/Library/Developer/CoreSimulator`): `xcrun simctl delete unavailable`, and `xcrun simctl erase all` while no simulator runs.
- Xcode's DerivedData, `~/.gradle`, Android emulators and package caches: move them to the other disk with each tool's own setting.

## Changes

- Small pull requests, one concern each.
- Commit messages: `<type>: <description>`, where type is one of `feat fix refactor docs test chore perf ci`.
- A wire-format change updates `protocol/PROTOCOL-v2.md` and every implementation together: Rust, Swift, Kotlin and the demo servers.
- A design decision gets a short ADR in `docs/adr/`.

## License

Each part keeps its license (see the README): `crates/`, `protocol/` and `scripts/` are MIT OR Apache-2.0, and `apple/` and `android/` are GPL-3.0-only.

**Contributor license agreement.** The apps are also offered under a commercial license, which is how they are sold on the App Store and Google Play. So a contribution to `apple/` or `android/` needs your permission for that. By opening a pull request that changes `apple/` or `android/`, you agree that:

1. you wrote the contribution, or have the right to submit it;
2. it is licensed under GPL-3.0-only, like the rest of that folder;
3. you also grant the author a perpetual, worldwide, non-exclusive, royalty-free license to use, change and distribute it under other licenses, including commercial ones.

Contributions outside `apple/` and `android/` are licensed MIT OR Apache-2.0 and need nothing more.
