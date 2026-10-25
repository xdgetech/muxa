# Contributing

Thanks for helping. muxa's one promise is that it never disturbs the desktop session it attaches to. Most of the process below protects that promise.

## Setup

- Rust stable (`rustup`; `rust-toolchain.toml` selects the components).
- tmux ≥ 3.2, bats, shellcheck, jq, python3.
- For the app: Xcode 16+.

```sh
make check   # must be green before you open a pull request
```

If your `shellcheck` lives elsewhere: `make check SHELLCHECK="<command>"`.

## Working with tmux

- Never experiment on your own tmux server. Use `make sandbox` or `tmux -L muxa-test-<name>`. The scripts in `scripts/` always pass `-L`.
- If you rely on a tmux behavior, prove it with a script in `scripts/repro/` and add a row to `docs/findings/tmux-control-mode.md`. CI runs these scripts on tmux 3.2a, 3.4 and the latest release.
- Target tmux objects by id (`%3`), never by name.

## Changes

- Small pull requests, one concern each.
- Commit messages: `<type>: <description>`, where type is one of `feat fix refactor docs test chore perf ci`.
- A wire-format change updates `protocol/PROTOCOL.md`, the fixtures, and every implementation together.
- A design decision gets a short ADR in `docs/adr/`.

## License

Each part keeps its license (see the README): `crates/`, `protocol/` and `scripts/` are MIT OR Apache-2.0, and `apple/` is GPL-3.0-only.

**Contributor license agreement.** The app is also offered under a commercial license, so a contribution to `apple/` needs your permission for that. By opening a pull request that changes `apple/`, you agree that:

1. you wrote the contribution, or have the right to submit it;
2. it is licensed under GPL-3.0-only, like the rest of `apple/`;
3. you also grant the author a perpetual, worldwide, non-exclusive, royalty-free license to use, change and distribute it under other licenses, including commercial ones.

Contributions outside `apple/` are licensed MIT OR Apache-2.0 and need nothing more.
