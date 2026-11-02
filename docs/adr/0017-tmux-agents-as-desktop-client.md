# 0017: tmux-agents as muxa's desktop client

## Status

Proposed.

## Context

- On the laptop the developer works in tmux itself: their own keys, navigation and vim. They SSH into the host and attach tmux there. A dedicated desktop app would take that away.
- tmux, tmux-agents and `muxa-bridge` all run on that host.
- Task records (ADR 0015) live only in muxa. A task started with tmux-agents' `prefix + N` is a session without a record. The iPad does not see it as a Task, and the laptop has no view of the Inbox.
- When the bridge launches through tmux-agents' `new.sh`, it tags the session with `@muxa_task` only after `new.sh` returns. `new.sh --task <id>` exists, but it names the session after the id and sets `@agent_task`, tmux-agents' own task name. A record id such as `t-7f3a2c` is a bad session name, and `@agent_task` is not `@muxa_task`.
- The built-in launcher copies tmux-agents' worktree and naming rules (ADR 0007). Only unit tests on each side pin them, so a change on one side goes unnoticed on the other.
- `muxa-cli task new` exits right after its `result`. The bridge it started ends with it, and so does the background wait that types the start prompt. A task created from the CLI never gets its prompt, and no note says so.

## Decision

### Task records stay the single source of truth

- tmux-agents never writes task records. It creates and lists Tasks through `muxa-cli` on the host, which starts a local `muxa-bridge`:

  ```sh
  muxa-cli --bridge 'muxa-bridge [--socket S]' task new --workspace W --title T --goal G [--repo R --branch B] --json
  muxa-cli --bridge 'muxa-bridge [--socket S]' tasks --json
  ```

- A Task started on the laptop is then the same record the iPad sees, created by the same code, with the same checks, `TASK.md` and start prompt.
- Rejected: tmux-agents reading and writing `~/.local/state/muxa/tasks/*.json` itself. The file format would become a second contract, and tmux-agents would need its own copy of `rev`, the directory fallback and the Inbox rule.
- Rejected: a socket or daemon. ADR 0015 lists when muxa gets a daemon; this is not one of those cases. One bridge per call costs well under a second.

### `--json`: the local contract

- `--json` prints one line of JSON on stdout. Messages and errors stay on stderr, and a failure exits non-zero. `docs/agents.md` ("Desktop tools") documents the shape.
- `tasks --json` prints `{"workspaces": [...], "truncated": bool, "tasks": [item...]}`.
- `task new --json` prints one item.
- An item is `{"group", "record", "session"?, "agent"?}`:
  - `record` is the wire `TaskRecord` (`protocol/PROTOCOL.md`).
  - `session` is `{id, name}` of the session whose `taskId` is the record's id.
  - `agent` is that session's agent state, only when it can be believed.
  - `group` is `needs-attention`, `ready-for-review`, `running`, `no-agent` or `closed`.
- `muxa-cli` computes the group with the rule of ADR 0015, in the same way as the iOS and Android apps. Items come in group order, newest change first within a group. tmux-agents only renders them. It has no copy of the rule.
- `task new` prints its line as soon as the record is saved. It then closes the bridge's input and waits for the bridge to exit.
- On end of input, the bridge now finishes what its client started before it exits: a launch still running, and the wait of up to 30 s for the start prompt (`task_records::Pending`, at most 100 s in all). This fixes the CLI bug above for every client.

### The Inbox and the jump on the desktop

- tmux-agents adds `prefix + T`: a popup with the Inbox groups (Needs attention, Ready for review, Running, No agent, Closed), from `tasks --json`.
- `Enter` on a Task switches *the desktop client that opened the popup* to its session, by session id.
- `prefix + N` opens a short form in a popup: workspace, repo, branch, title and goal. When muxa is available, the form creates a Task through `task new --json`, then switches the desktop client to its session.
- Without `muxa-cli`, or with `@agents_muxa off`, `prefix + N` is the plain repo-and-branch prompt of today, and `prefix + T` says muxa is not installed. With `muxa-cli` but no muxa workspace, the form asks only for repo and branch and makes a plain task.
- A popup does not change any window or pane. Moving a desktop client is tmux-agents' job. The bridge still never switches, selects or resizes (invariant 2).

### The link is made at creation

- `new.sh` gets `--muxa-task <id>`. It sets the session option `@muxa_task` before it builds the layout. The session name stays the branch's name. `--task` keeps its meaning (tmux-agents' task name, `@agent_task`).
- On an existing session for the branch, `new.sh` sets `@muxa_task` only when the session has none. The bridge then still sees another Task's tag and refuses, as ADR 0015 says.
- tmux-agents announces the flag in the global option `@agents_features`, a space-separated list that contains `muxa-task`. The bridge passes `--muxa-task` only when the list has it. Older tmux-agents reject unknown flags, so they get today's call.
- `@agents_features` is how tmux-agents announces any later addition to the contract. For example, the per-workspace agent environment of ADR 0016 can add its own word.
- The bridge still sets `@muxa_task` after launch, for older tmux-agents and the built-in launcher. Setting the same value again changes nothing.
- This supersedes ADR 0015's "a short id can later be passed to tmux-agents' `new.sh --task`".

### Worktree and naming rules: tmux-agents owns them

- tmux-agents' `new.sh` owns the worktree path (`<repos dir>/.worktrees/<repo>-<branch with / as ->`), the branch checkout rules and the session name. The desktop shows them, and tmux-agents must work without muxa.
- muxa's built-in launcher is a fallback copy for hosts without tmux-agents.
- A contract test in muxa prevents drift. `crates/muxa-bridge/tests/tmux_agents.rs` runs the real `new.sh` and the built-in launcher on the same branches. It compares the worktree path and the session name, and it checks `--muxa-task`. It runs in `make test-tmux` when `MUXA_TMUX_AGENTS` names a tmux-agents checkout, like `MUXA_TEST_PLUGIN`.
- tmux-agents' own bats tests keep pinning its side.

### The repos stay separate

- TPM installs a plugin by cloning its repo (`git clone https://github.com/<owner>/<repo>`, `tpm/scripts/install_plugins.sh`). It then runs the `*.tmux` files at the root of that clone (`tpm/scripts/source_plugins.sh`). tmux-agents inside muxa would make TPM clone the whole muxa repo, with Rust, Swift and Android, and would need a `*.tmux` file at muxa's root.
- tmux-agents changes layouts and has a sidebar. muxa's first promise is not to change layouts. The rejection in ADR 0007 stands.
- The contract has two documents, one per direction:
  - muxa → tmux-agents: tmux-agents' README, "For other tools": `@agents_scripts`, `@agents_features`, `new.sh`, `close.sh` and the tags.
  - tmux-agents → muxa: muxa's `docs/agents.md`, "Desktop tools": the `--json` output and `@muxa_task`.

## Consequences

- The laptop and the iPad show the same Tasks and states. Neither is a second store.
- tmux-agents needs `muxa-cli` and `muxa-bridge` on the host for Tasks. Its plain sessions work without them.
- Every `prefix + T` starts one bridge, which attaches as a control client and leaves. tmux-agents already ignores control clients (`tests/control.bats`).
- This supersedes ADR 0007's consequence "A change there must be copied here. Unit tests pin both rules." A change in tmux-agents still has to be copied, but the contract test now fails until it is.
- The Inbox rule now has three copies: Swift, Kotlin and `muxa-cli`. A shared fixture for it is a follow-up.
- Rejected: moving the Inbox rule into the bridge as a new message. That would be a protocol change, and the apps already compute the rule.
