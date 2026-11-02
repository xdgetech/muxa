# 0019: A host daemon for the Task Loop

## Status

Proposed. Partly supersedes ADR 0003 ("There is no daemon and no listening port") and the "No daemon now" section of ADR 0015. The SSH exec bridge stays; no network port is added.

## Context

- The vision says muxa owns the Task loop: which Task runs, and when it counts as done (`docs/vision.md`). Today nothing checks the agent's work. The user reads it, runs the tests, and types the next prompt.
- A loop must run while no client is connected. iOS closes the SSH connection in the background (ADR 0003), and the bridge ends with it. That is exactly when a loop is useful. ADR 0015 named this as the first trigger for a daemon.
- Two bridges (an iPad and an iPhone) can run at once. A loop inside each bridge would run twice and type every prompt twice.
- The bridge already waits up to 100 s after its client leaves, to type the start prompt (`task_records::Pending`, ADR 0017). Runs are recorded only while a bridge acts (ADR 0018). Both are gaps that a long-lived process closes.
- The record store is already safe for several writers: `rev` plus a lock file per record makes each read-compare-write one step (`records/store.rs`).
- Approval gates: muxa never approves on the agent's behalf. A permission, question or plan menu is answered by a human (ADR 0009).
- The user decided: the loop runs in a long-lived daemon on the host, started by launchd (a LaunchAgent) on macOS and `systemd --user` on Linux. It opens no network port. It listens only on a Unix socket in the user's state directory (mode 0700).

## Decision

### 1. Process and install

**A subcommand of `muxa-bridge`, not a new binary.**

| Command | What it does |
|---|---|
| `muxa-bridge daemon run [flags]` | Runs the daemon in the foreground. launchd and systemd run this. |
| `muxa-bridge daemon install [flags]` | Writes the LaunchAgent plist or the systemd user unit, then loads and starts it. No sudo. |
| `muxa-bridge daemon uninstall` | Stops it and removes the file. Task records stay. |
| `muxa-bridge daemon status` | Says whether it runs, its version, its socket, and its log file. |
| `muxa-bridge daemon install --print` | Prints the file it would write, and changes nothing. |

- Why a subcommand: the daemon needs the record store, the task code, the agent adapters, `find_tmux` and the tmux client. They all live in `muxa-bridge` today. A second crate would mean moving them into a library first. One binary also means one install and one version on the host.
- The SSH forced command stays `muxa-bridge` with no subcommand. sshd ignores the client's command when `command=` is set, so a client key cannot run `daemon install`. Installing is a host decision, made by the user in a shell.
- `install` takes the same flags as serve mode (`--tmux`, `--socket`, `--state-dir`, `--workspaces`, `--agent-dir`, `--task-launcher`, `--task-command`) and writes them into the file. It writes the absolute path of the running binary and of the tmux it found. launchd gives a minimal `PATH`, so nothing is looked up later.
- `install` also writes the `PATH` of the shell that runs it into the file, for the check command (section 3). This is the only environment value it copies. The user sees it in the file.
- macOS: `~/Library/LaunchAgents/tech.xdge.muxa.daemon.plist`, with `RunAtLoad`, `KeepAlive {SuccessfulExit: false}`, `ThrottleInterval 10`. Loaded with `launchctl bootstrap gui/<uid>`, removed with `launchctl bootout`.
- Linux: `~/.config/systemd/user/muxa-daemon.service`, with `Restart=on-failure`, `RestartSec=5`. Started with `systemctl --user enable --now`. A user service stops at logout unless lingering is on. `install` prints the `loginctl enable-linger` hint; it does not run it.

**Restart and a single instance.**
- launchd or systemd restarts the daemon when it crashes. A clean exit (exit 0) is not restarted.
- The daemon holds an exclusive `flock` on `daemon.lock` while it runs. The kernel releases it when the process dies, so a crash leaves no stale lock. A second daemon that cannot take the lock exits 0 and is not restarted.
- After it has the lock, the daemon removes a leftover `daemon.sock` and binds a new one.

**Files.** New flag `--daemon-dir DIR`, default `$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`. This is the parent of the existing `tasks/` directory. It holds `daemon.sock`, `daemon.lock` and `daemon.log`. The directory has mode 0700 and the socket 0600. A Unix socket path has a limit of about 104 bytes on macOS; the daemon refuses a longer one with a clear message.

**Finding tmux.** The bridge's `find_tmux`, with the absolute path that `install` wrote. The daemon talks to the server named by `--socket`, as the bridge does.

**When tmux is not running.**
- The daemon never starts a tmux server. It never runs `new-session` except for a Task launch that a client asked for.
- Without a server it still serves record requests. Every Task's loop waits: no agent state means no action (invariant 6). It looks for a server again every 10 seconds.
- When the server ends while the daemon is attached, the daemon drops its client and looks again later.

**Its tmux client.**
- The daemon reuses `muxa-tmux`'s control client, attached with `-f ignore-size,no-output`. `no-output` means no `%output`, so no pane bytes reach the daemon.
- It attaches only while it has work: an active loop, a launch, or a prompt wait. It detaches after 60 seconds without work. A plugin's `client-attached` hook then runs once per stretch of work, not every 2 seconds.
- When the session it is attached to is destroyed, tmux detaches it (`docs/findings/tmux-control-mode.md`). The daemon attaches again on the next tick.
- New tmux behavior gets a repro script, `scripts/repro/daemon-client.sh`: with `no-output`, no `%output` arrives; a long-lived `ignore-size` client next to a desktop client changes no layout over several minutes; detaching changes nothing.

**Logs.**
- The daemon writes `daemon.log` itself, and rotates it at 1 MiB, keeping one old file. launchd's `StandardErrorPath` (`daemon.err`) and the systemd journal catch only start-up failures.
- A line carries ids, paths, states, exit codes and durations. Never a title, goal, note, check command, check output, env value or prompt text (invariant 5). The default level is `info`.

### 2. One writer, one code path

- **The daemon is the usual writer of task records.** A bridge sends every record request to the daemon when the daemon runs: `list-tasks`, `create-task`, `update-task`, `restart-agent`, `start-loop`, `stop-loop`.
- **The socket speaks protocol v1.** The same `Request` and `Message` types, as JSON lines. The daemon sends `hello` first, with its version and capabilities. It answers the requests above, and `unsupported` to every other request.
- One bridge opens one socket connection, at start, with a 200 ms timeout. Request ids pass through unchanged: the bridge relays the daemon's `task-saved`, `task-list` and `result` to its client as they come.
- **Fallback.** When no daemon answers, the bridge runs the same handlers in its own process, with direct writes and `rev`, as today.
- **Version.** The bridge uses the daemon only when the daemon's version is the bridge's own. After an upgrade, an old daemon is not used until `daemon install` (or `daemon restart`) replaces it. The bridge logs a warning. This keeps two versions of the handlers from serving one client.
- **A daemon that stops mid-request** is an `internal` error: "the muxa daemon stopped; try again". The bridge does not retry locally, because the daemon may have written already. Later requests on that connection use the fallback.
- **How the two paths stay one.**
  1. All record requests go through one function, `task_records::handle(env, request) -> messages`. The bridge's fallback calls it. The daemon's socket server calls it. The only branch is in the bridge: `Records::Daemon(socket)` or `Records::Local(env)`.
  2. The `tests/task_records.rs` suite runs twice: once with a daemon on a temp socket, once without. The same assertions hold for both.
- **Correctness still rests on `rev` and the lock.** "Single writer" is a routing rule, not a promise. A bridge that falls back while a daemon starts writes safely. A loop write and a user's `update-task` that race get `conflict`, and the loop retries as `handoff::write` does today.
- **The daemon owns the waits.** A forwarded `create-task` or `restart-agent` leaves its prompt wait in the daemon. It no longer ends with a bridge, and the 100 s limit of `Pending::finish` does not apply to it.
- **`tasks-changed`.** The daemon sends this message, with no fields, to every connected bridge after it writes a record. The bridge relays it. A client lists the Tasks again, as it does on `sessions-changed`. Loop progress shows without polling.

### 3. The loop

A Task can have one optional loop:

| Field | Meaning |
|---|---|
| `check` | A shell command, run with `/bin/sh -c` in the Task's `dir`. |
| `maxAttempts` | Failed checks before the loop stops. 1 to 10, default 3. |
| `timeout` | Seconds one check may run. 10 to 3600, default 600. |

**When the check runs.** The trigger is the agent finishing a turn, as its adapter reports it:
1. The loop is `armed` from the time it starts, or from the time it last typed a prompt (`armedAt`).
2. On each 2-second tick, the daemon reads the Task's agent pane with `handoff::agent_pane`.
3. The check runs when the state is `idle`, its `since` is at or after `armedAt`, and the same state is still there 5 seconds later. A state with no `since` never triggers (ADR 0018: a state with no time is not new).

**How the check runs.**
- A subprocess of the daemon, in its own process group. It is never typed into a pane.
- Its environment: the daemon's, the `PATH` from install, and the Task workspace's `agent.env` (ADR 0016). The agent runs the same tests with that environment, so the check gets the same accounts.
- stdout and stderr go to `.muxa/checks/<attempt>.log` in the Task's `dir`, at most 8 MiB. Past that, the daemon drops the rest and ends the file with one line that says so. The newest 20 logs are kept.
- After each check, the daemon writes `.muxa/CHECK.md`: the attempt, the exit code, the duration, and the last 200 lines of the log in a code block, with terminal escapes removed. The Reader lists it next to `TASK.md` and `HANDOFF.md`.
- At `timeout`, the daemon kills the process group.
- `TASK.md`'s header gets a `## Check` section: the command, and "muxa runs this after each of your turns. The Task is ready for review when it exits 0." The agent can run it itself.

**What happens after a check.**

| Result | The loop |
|---|---|
| Exit 0 | State `passed`. Task status `open` becomes `review`. Note: "The check passed on attempt n." The loop ends. |
| Non-zero, attempts left | Types the fail prompt, then `armed` again. |
| Non-zero, limit reached | Stops with reason `limit`. |
| Timed out | Stops with reason `timeout`. |
| The agent left `idle` while the check ran | The result is dropped and the attempt does not count. The log stays. `armed` again. |

The fail prompt is fixed text. Only the attempt number changes:

```
The check failed. Its output is in .muxa/checks/<n>.log. Fix the cause, then end your turn.
```

It goes through the existing prompt path (`handoff::send_prompt`): `send-keys -l`, then Enter, only while the agent is `idle`, within 30 seconds. Otherwise the loop stops with reason `not-idle`.

**When the loop stops and asks for the user.** On any of these, the loop stops at once and does nothing more in the pane:

| Reason | When |
|---|---|
| `limit` | `maxAttempts` failed checks. |
| `timeout` | A check ran longer than `timeout`. |
| `waiting` | The agent is `waiting`: a permission, a question or a plan. |
| `error` | The agent's state is `error`. |
| `no-agent` | No linked session, no live agent pane, or `shellForeground: true`. |
| `not-idle` | The fail prompt could not be typed within 30 seconds. |
| `status` | The user set the Task's status to anything but `open`. |
| `user` | `stop-loop`. |

- The note says why, in one sentence, with no check output.
- Every reason except `user` and `status` puts the Task in **Needs attention** (section 5).
- `start-loop` starts it again. The attempt count goes on from where it stopped, unless the request resets it.

**What "done" means.** The check exits 0 after a turn of the agent. The loop then moves the Task to `review`, never to `done`. A human reads the result and sets `done`.

**What the loop never does.**
- It never answers a menu. It sends no `1`, `y`, `Esc` or Enter to a `waiting` agent, and never uses the keys of an approval card (ADR 0009). `waiting` stops the loop.
- It never restarts the agent. `restart-agent` stays a user action.
- It never pushes, merges, closes a session or removes a worktree.
- It never reads the pane's screen. All state comes from adapters (invariant 6).

**Daemon restart.** Loops are in the records, so they survive. An `armed` loop goes on. A `checking` loop goes back to `armed`, and the attempt does not count. On a clean stop the daemon kills the check's process group first. After a crash the check may still run; its result is never read.

**A known risk.** The user can have typed half a line into the agent's input box while the agent is `idle`. The fail prompt is then added to that text. muxa cannot see the draft without reading the screen. The 5-second settle makes this less likely; input leases (`docs/plan.md`, Later) would fix it.

### 4. Who may set a check command

- **Plainly: setting a check means running a command on the host as the user.** Any client that holds the SSH key can do it, through `create-task` or `start-loop`.
- This grants nothing new. The key can already type into any pane, and a shell pane runs any command (ADR 0005, 0015). The bridge already starts agents that run commands.
- What is new is the path: a command runs with no pane to watch it. So the host decides whether the path exists, per workspace, in `workspaces.toml`, which clients cannot write:

  ```toml
  [workspace.loop]
  checks = ["make check", "cargo test --workspace"]   # or checks = "any"
  ```

  - No `[workspace.loop]` table: the workspace has no loops. `start-loop` and `create-task` with a loop are `bad-request`, with a message that names the table.
  - A list: the check must be one of the strings, exactly.
  - `"any"`: any command, at most 4096 bytes and with no control character.
- `list-tasks` sends each workspace's allowed checks, so the apps can offer a picker instead of a text field.
- Installing the daemon is itself the host's opt-in. Without a daemon there are no loops.
- The check command is stored in the record. It is never logged and never placed on a tmux command line.

### 5. Wire, CLI and apps

**Protocol** (compatible within v1):
- Capability `task-loop`: the bridge reached a daemon of its own version, and loops work. A bridge without a daemon does not send it.
- `create-task` gets optional `loop {check, maxAttempts?, timeout?}`.
- Request `start-loop {id, task, loop {check, maxAttempts?, timeout?}, reset?}`. `task-saved` comes before the `result`.
- Request `stop-loop {id, task}`. `task-saved` comes before the `result`.
- Message `tasks-changed`, with no fields.
- Record field `loop`, absent when the Task has none: `{check, maxAttempts, timeout, state, attempts, since, reason?, lastCheck? {attempt, exit?, seconds, at}}`.
  - `state`: `armed`, `checking`, `passed` or `stopped`. Treat an unknown state as one with no known meaning.
  - `reason` only when `stopped` (section 3). Treat an unknown reason the same way.
  - `exit` is absent when the check timed out.
- `task-list`'s `workspaces[]` get optional `checks`: a list of strings, or `"any"`. Absent means no loops in that workspace.
- The record stays `version` 1. `loop` is additive, and older bridges keep it on rewrite (ADR 0015).
- A `start-loop` or a `create-task` with `loop` when no daemon runs is `unsupported`: "the Task Loop needs the muxa daemon. On the host, run muxa-bridge daemon install."
- `PROTOCOL.md`, `protocol/fixtures`, and the Rust, Swift and Kotlin types change together.

**CLI.**
- `muxa-cli task new ... --check CMD [--max-attempts N] [--check-timeout S]`
- `muxa-cli task loop ID --check CMD [...] [--reset]` and `muxa-cli task stop-loop ID`
- `muxa-cli tasks` shows the loop state and attempts. `--json` carries the record as is.

**Apps (iOS and Android).**
- Task detail: a Loop row with the state, "attempt n of max", the last check's result and age, and the stop reason. Buttons: Stop loop, Start loop, Open check log. Open check log opens `.muxa/CHECK.md` in the Reader.
- New Task sheet: "Run a check after each turn", with a picker from the workspace's `checks`, or a text field for `"any"`.
- Without `task-loop`, the controls are hidden. A line says the host needs the muxa daemon.
- On `tasks-changed`, list the Tasks again.

**The Inbox rule** gets one row before the others: **Needs attention** also holds a Task whose loop is `stopped` with a reason other than `user` or `status`. The rule has three copies (Swift, Kotlin, `muxa-cli`). This change adds the shared fixture that ADR 0017 left as a follow-up: `protocol/fixtures/inbox/`, inputs and expected groups, decoded by all three test suites.

### 6. What the daemon enables later

Not built now. Each gets its own ADR when it is needed.
- **Runs while no client is connected.** The daemon can record an agent run's start and end for every Task, and close ADR 0018's gaps.
- **A scheduler.** Start a Task at a time, or start the next Task in a workflow when one passes (vision phase 4).
- **Push while the app is closed.** The daemon sees every state change. It could send a notice to a relay that holds the APNs and FCM credentials (ADR 0008). That is outbound only; the daemon still listens on no port.

Out of scope, explicitly:
- Any network listener, web UI or remote API.
- Approving, denying or answering anything for the agent. Setting a Task to `done`. Pushing, merging or closing.
- Restarting an agent automatically.
- Running checks in a sandbox or a container.
- Several hosts, and Windows.

### 7. Testing

- **No launchd or systemd in tests.**
  - `install --print` is checked against golden plist and unit files in unit tests.
  - The `launchctl` and `systemctl` calls sit behind a small trait; unit tests use a fake.
  - Loading the real LaunchAgent is a manual check by the developer, like `make install-ipad`.
- **`make test-tmux` runs a real daemon.** A test starts `muxa-bridge daemon run --socket muxa-test-<name> --state-dir <tmp>/state --workspaces <tmp>/workspaces.toml --daemon-dir <short tmp>` as a child process. The daemon dir is short, under `/tmp`, because of the socket path limit. The test kills the daemon at the end. No test touches the default server (invariant 1).
- **The record suite runs on both paths**, with and without a daemon (section 2).
- **A fake agent**: a shell script in the agent pane, in the style of the sandbox's `scripts/lib/fake-agent.sh`, which sets no state today. It reads a line, sets `@agent_state running` and `@agent_state_at` on its pane, writes a marker file, then sets `@agent_state idle` with a new time. It can also set `waiting` or `error` on request.
- **Fake checks**:
  1. `test -f fixed`: fails once, the fake agent writes `fixed`, then it passes. The Task becomes `review`, with 2 attempts.
  2. `false`: stops with `limit` after `maxAttempts`, and the fail prompt was typed `maxAttempts - 1` times.
  3. `sleep 60` with `timeout 10`: stops with `timeout`, and the process group is gone.
  4. The fake agent goes `waiting`: the loop stops with `waiting`, and the pane shows nothing typed after that.
  5. The daemon is stopped during a check and started again: the check's process group is gone, the loop is `armed`, and the attempt did not count.
- Every loop test records the layout with `scripts/layout-snapshot.sh` before and after. The diff is empty (invariant 2).
- A test greps `daemon.log` for the goal, the check command and a marker in the check output. None appears (invariant 5).
- Socket permissions (0700 dir, 0600 socket) and the single instance (a second daemon exits 0) are tested. The peer uid check is a unit test.
- The loop's decisions are a pure function, `step(state, observation) -> (state, action)`, unit-tested without tmux.

## Consequences

- A Task can fix itself until a check passes, while the phone is off, and stops for the user at every gate.
- The host runs a resident process, and the user installs it by hand. Without it, muxa works as today, with no loops.
- After an upgrade, loops stop working until the daemon is restarted on the new version. `daemon status` says so.
- The record format grows `loop`. Older bridges keep it, but cannot show it.
- The Inbox rule changes in three clients at once, now pinned by a shared fixture.
- A check runs with the user's rights and the workspace's tokens. The workspace's `checks` list is the host's control.
- Rejected: **the daemon inside tmux** (a hidden session or window). It would show in session lists and on the desktop, end with `kill-server`, and could not notice that tmux is gone. Its environment would be the tmux server's, and nothing would restart it.
- Rejected: **agent hooks** (for example a Claude Code `Stop` hook that runs the check). They are per agent and Codex has no equal. They live in config directories that muxa would have to edit, one per workspace (ADR 0016). The agent's own process would run the loop, which muxa owns (vision). A hook cannot stop and escalate across agent restarts. Hooks can still be a faster *signal* of a turn's end later.
- Rejected: **no daemon**, the loop in the per-connection bridge. It ends when iOS closes the connection, which is when a loop is useful. Two bridges would run two loops and type twice.
- Rejected: **a separate `muxa-daemon` binary.** See section 1.
- Rejected: **the daemon as the only writer**, with no fallback. Without the daemon, Tasks would stop working, and ADR 0015's store already makes several writers safe.
- Rejected: **a long-lived control client with output on.** Pane bytes would reach a process that needs none of them.
- Rejected: **auto-resuming the loop after the user answers a menu.** It may be right later, but stopping is the plain gate for now (open question).

## Implementation plan

Each phase is verified on its own. They are ordered by risk. P1 and P2 are the risky ones: a resident process, and typing into panes with no user watching.

| Phase | What | Verified by | Depends on |
|---|---|---|---|
| P1. Daemon, socket, one writer | `daemon run/install/uninstall/status`, `--daemon-dir`, the lock, the socket and its modes, the peer uid check, the version check. `task_records::handle` as the one entry point. The bridge's forward and fallback. Prompt waits in the daemon. `tasks-changed`. The lazy `ignore-size,no-output` client and `scripts/repro/daemon-client.sh` with its findings row. | `make check`; `make test-tmux` with the record suite on both paths; `make repro`; `install --print` golden files; the developer loads the LaunchAgent once by hand. | none |
| P2. Loop engine | `step()` and its unit tests; the check runner (process group, timeout, log cap, `CHECK.md`); `TASK.md`'s `## Check`; `[workspace.loop]` in `workspaces.toml`; the tick in the daemon. | `make check`; `make test-tmux` with the fake agent and the five fake checks; layout snapshot; the log grep. | P1 for the integration; `step()` and the runner can start in parallel with P1. |
| P3. Protocol and CLI | `PROTOCOL.md`, fixtures (including `inbox/`), Rust types, `start-loop`, `stop-loop`, `loop` on `create-task`, `checks` on workspaces, `muxa-cli` flags and output; Swift and Kotlin decoding. | `make check`; `make test-apple`; `make test-android`. | Can start right after this ADR is accepted, in parallel with P1 and P2. The handlers land with P2. |
| P4. iOS UI | Loop row, New Task option, Inbox row, Open check log; the demo bridge gets loop data. | `make test-app`, `make test-app-iphone`. | P3 fixtures. Parallel with P5. |
| P5. Android UI | The same as P4. | `make test-android-app`, `make lint-android`. | P3 fixtures. Parallel with P4. |
| P6. Docs | `docs/plan.md`, `SECURITY.md` (the daemon, the socket, check commands), README install steps, `docs/agents.md`. ADR 0019 to Accepted. | Review. | P1 to P5. |

## Open questions for the user

1. **`done` from tmux-agents.** Does tmux-agents' `done` mean "the turn ended" or "the agent exited"? The loop triggers only on `idle` for now.
2. **`waiting` stops the loop.** Should the loop instead resume by itself once the user has answered and the agent is `running` again?
3. **Check opt-in.** A workspace without `[workspace.loop]` has no loops. Is that the right default, or should `"any"` be the default, since the key is already shell-equivalent?
4. **macOS without a GUI login.** A LaunchAgent in `gui/<uid>` runs only while the user is logged in at the Mac. Is a Mac reached only over SSH a case to support (`user/<uid>` domain)?
5. **The check's `PATH`.** It comes from the shell that ran `install`. The alternative is the tmux server's global environment, which is what the panes get.
6. **Status on pass.** `open` becomes `review`. Should the loop leave the status alone and only write the note?
