# Running Workers W0 checks

These checks do not enable workers. Run from the muxa checkout. Do not run `make install`, restart the live daemon, or use the default tmux server.

## Automated component checks

```sh
bash scripts/repro/worker-respawn.sh
cargo test -p muxa-bridge --bin muxa-bridge registry_rematches_replaced_processes_in_the_same_pane
cargo test -p muxa-bridge --bin muxa-bridge record_lock_does_not_serialize_external_restart_effects
cargo test -p muxa-bridge --test lifecycle_lock_probe
```

Expected: two tmux integration tests, one test from each unit-test command, and the lifecycle lock probe pass. The lock probe holds ownership for eleven seconds, tests competing processes through directory aliases, and verifies release after owner death. The CAS test passes by demonstrating that the existing record lock does **not** prevent two external effects; it does not certify safe worker restarts.

For the broader regression checks:

```sh
make check
make test-tmux
make repro
```

If mise has no selected ShellCheck version, use `mise exec shellcheck@0.11.0 -- make check`. Cross-version evidence requires the existing CI matrix on Linux tmux 3.2a/3.4 and macOS Homebrew tmux; a local 3.7c pass does not complete that gate.

## Native startup, in an authenticated terminal

Use a throwaway repository and worktree, plus a non-repo workspace Task directory. Create new native definitions only; do not copy credentials or existing profiles. Do not send a prompt or accept any action that starts work. Stop at onboarding/login and record the prerequisite if authentication is missing.

1. Check `claude --version`, `codex --version` and each `--help`. Confirm this installation supports `claude --agent NAME` and `codex --profile NAME`.
2. Create a new Claude native role named `muxa-w0-fixture`, first in the native user agents directory, then separately in the throwaway worktree’s `.claude/agents/`. Give it only a description, read access and the instruction to wait for a user request. No hooks or initial prompt. Review the newly authored definition before launch.
3. Create a new `muxa-w0-fixture.config.toml` in the existing Codex config directory. Start with a comment-only file to separate profile loading from optional setting compatibility. Native profile syntax: [official configuration documentation](https://learn.chatgpt.com/docs/config-file/config-advanced). Do not change the base config.
4. From the throwaway Task worktree, launch each CLI interactively with the new name, without a positional prompt. Check selection using native UI/config inspection without making a model turn. If the CLI does not expose selection before a turn, record it as unverified.
5. Repeat from the non-repo workspace Task directory. For Claude, test project-role discovery separately from user-role discovery; a project definition in the main checkout may not exist in a worktree. Repeat with a custom config home only if one is already independently authenticated; do not copy login files.
6. Repeat with an intentionally missing role/profile name. Record error, onboarding, or observable fallback. An onboarding screen alone does not establish whether the native reference is valid.
7. For muxa adapter checks, run each native CLI in a new `tmux -L muxa-test-w0-native` server, launched from a shell with the intended config environment. Point the built bridge at **that socket** and use the CLI `list` command. Confirm only the reported agent id/state; unknown is acceptable evidence, invented idle is not. Test Claude → Codex → Claude in the same test pane and check that old state/session binding is not reused. Keep this rematching case unverified until it has been directly observed.
8. Quit the probe CLI, kill only the isolated test server, and remove only the fixture definitions you created. Do not retain terminal/transcript contents in test reports.

Record CLI/tmux version, directory class, config class, intended native selection, confirmed/unverified selection, adapter agent id/state, and outcome. Account names, paths, terminal output and transcripts do not belong in the report. A visible worker label does not prove the native role took effect.

## Restart concurrency and crash cases

R01–R10 in [the W0 report](workers-w0.md) use the lifecycle claim/recovery design in proposed ADR 0024 and require W2 integration hooks. R11–R16 in that ADR extend the crash/delivery acceptance matrix. They cannot be run against a worker API today: W1 deliberately rejects worker execution. Do not mark them passed from the synthetic component tests. The next engineering step is to settle that transaction boundary and make those acceptance cases executable before enabling the capability.
