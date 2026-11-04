# 0032: muxa for tmux lives in this repo

## Status

Proposed. Replaces the section "The repos stay separate" of ADR 0017. The rest of ADR 0017 stands: the `muxa-cli --json` contract, `--muxa-task`, `@agents_features`, and the plugin never writing records.

## Context

- tmux-agents is now a muxa client, like the iOS and Android apps. Its New Task form, Inbox popup and ask sessions (`prefix + N`, `T`, `A`) exist only to show muxa's records.
- A change to `muxa-cli --json` or to a session tag had to land in two repos. The contract test ran only when someone set `MUXA_TMUX_AGENTS` to a checkout by hand, so drift showed up late.
- ADR 0017 kept the repos apart for two reasons:
  1. TPM clones a whole repo and runs the `*.tmux` files at its root.
  2. The plugin changes layouts, and muxa promises not to.

## Decision

- tmux-agents moves into `tmux/`, with its history (`git subtree add`). Its name becomes **muxa for tmux**. The GitHub repo `xdgetech/tmux-agents` is archived and points here.
- The names other tools rely on stay as they are: `agents.tmux`, `@agents_*`, `@agent_*`, `new.sh`, `close.sh` and `TMUX_AGENTS_SOCKET`. Renaming them would break the bridge's launcher and older configs for no gain.
- **Install without TPM.** The user loads `run-shell '<muxa>/tmux/agents.tmux'` from a muxa checkout. No `*.tmux` file is added at muxa's root. Packaging the plugin with a muxa release is a follow-up.
- **The layout promise is the bridge's, not the plugin's.** Invariant 2 covers what the bridge and the apps do to the user's tmux. The plugin lays out only the sessions it creates, and only when the user loads it. The bridge never loads it.
- `make test-tmux-plugin` runs the plugin's bats tests and the contract test against `tmux/`. CI runs it in its own macOS job, because the plugin needs tmux 3.4 or newer and fzf, and CI's Ubuntu 22.04 has tmux 3.2. `make lint-sh` covers `tmux/`.
- The plugin's README stays the contract document for the direction muxa → plugin ("For other tools"). `docs/agents.md` ("Desktop tools") stays the one for plugin → muxa.

## Consequences

- One commit can change `muxa-cli --json`, the plugin and the apps together.
- The built-in launcher still copies `new.sh`'s worktree and naming rules (ADR 0007), because the bridge must work where the plugin is not loaded. The contract test now runs on every CI build. Using `tmux/scripts/new.sh` directly would need the scripts installed with the bridge. That is a separate decision.
- Agent state has two writers: the plugin's hooks (`tmux/hooks/`) set `@agent_state`, and the bridge's adapters (ADR 0005, 0008) read the agents directly. Unifying them is a follow-up.
- muxa's history now holds tmux-agents' commits.
- Rejected: a git submodule. It keeps two repos and two histories, and a contract change still needs two commits.
