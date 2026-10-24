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

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as is the project.
