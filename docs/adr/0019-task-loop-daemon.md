# 0019: A host daemon for the Task Loop

## Status

Proposed. It partly supersedes:
- ADR 0003: "There is no daemon and no listening port". The SSH exec bridge stays, and no network port is added.
- ADR 0015, "No daemon now".
- ADR 0015, "Task status is set by the user". The loop may move a Task from `open` to `review` (section 3).

## Context

- The vision says muxa owns the Task loop: which Task runs, and when it counts as done (`docs/vision.md`). Today nothing checks the agent's work. The user reads it, runs the tests, and types the next prompt.
- A loop must run while no client is connected. iOS closes the SSH connection in the background (ADR 0003), and the bridge ends with it. That is exactly when a loop is useful. ADR 0015 named this as the first trigger for a daemon.
- Two bridges (an iPad and an iPhone) can run at once. A loop inside each bridge would run twice and type every prompt twice.
- The bridge waits up to 100 s after its client leaves, to type the start prompt (`task_records::Pending`, ADR 0017). Runs are recorded only while a bridge acts (ADR 0018). A long-lived process closes both gaps.
- The record store is already safe for several writers: `rev` plus a lock file per record makes each read-compare-write one step (`records/store.rs`).
- tmux-agents reports a finished Claude Code turn as `done`, not `idle`. Its `Stop` hook sets `done` (`tmux-agents/hooks/claude-hook.sh`). `state.sh` turns `done` into `idle` only when a person's client watches the window, and a control client never counts (`scripts/state.sh`, `scripts/lib.sh` `watched_windows`). While the user is away, a finished turn stays `done`.
- Approval gates: muxa never approves on the agent's behalf. A permission, question or plan menu is answered by a human (ADR 0009).
- The user decided:
  - The loop runs in a long-lived daemon on the host. On macOS launchd starts it as a LaunchAgent. On Linux `systemd --user` starts it.
  - It opens no network port. It listens only on a Unix socket in the user's state directory (mode 0700).
  - The host Mac is always logged in at the GUI, so a `gui/<uid>` LaunchAgent is enough.

## Decision

### 1. Process and install

**A subcommand of `muxa-bridge`, not a new binary.**

| Command | What it does |
|---|---|
| `muxa-bridge daemon run [flags]` | Runs the daemon in the foreground. launchd and systemd run this. |
| `muxa-bridge daemon install [flags]` | Writes the LaunchAgent plist or the systemd user unit, then loads and starts it, and waits up to 5 s until it answers with this version. No sudo. |
| `muxa-bridge daemon install --print` | Prints the file it would write, and changes nothing. |
| `muxa-bridge daemon stop` | Stops every loop (`stopped`, reason `user`), then stops the daemon. It stays installed and starts again at the next login. |
| `muxa-bridge daemon restart` | Restarts the daemon, for example after an upgrade, and waits up to 5 s until it answers with this version. Otherwise it fails and names the log. Loops go on. |
| `muxa-bridge daemon uninstall` | Like `stop`, then removes the file. Task records stay. |
| `muxa-bridge daemon status` | Running or not, its version, socket and log file. On macOS, "not loaded" when the LaunchAgent is not loaded in `gui/<uid>`. |

- Why a subcommand: the daemon needs the record store, the task code, the agent adapters, `find_tmux` and the tmux client. They all live in `muxa-bridge` today. A second crate would mean moving them into a library first. One binary also means one install and one version on the host.
- The SSH forced command stays `muxa-bridge` with no subcommand. sshd ignores the client's command when `command=` is set, so a client key cannot run `daemon install`. Installing is a host decision, made by the user in a shell.
- `install` takes the same flags as serve mode (`--tmux`, `--socket`, `--state-dir`, `--workspaces`, `--agent-dir`, `--task-launcher`, `--task-command`) and writes them into the file. It writes the absolute path of the running binary and of the tmux it found. launchd gives a minimal environment, so nothing is looked up later.
- `install` copies these environment values of its shell into the file, and no others:
  - `HOME`, which the default paths use;
  - `TMUX_TMPDIR`, when set, so `-L` finds the same server socket as the user's shell;
  - `PATH`, only as the check's fallback (section 3).
- macOS: `~/Library/LaunchAgents/tech.xdge.muxa.daemon.plist`, with `RunAtLoad`, `KeepAlive {SuccessfulExit: false}` and `ThrottleInterval 10`. It is loaded with `launchctl bootstrap gui/<uid>` and removed with `launchctl bootout`. Running without a GUI login (`user/<uid>`) is out of scope.
- Linux: `~/.config/systemd/user/muxa-daemon.service`, with `Restart=on-failure` and `RestartSec=5`. It is started with `systemctl --user enable --now`. A user service stops at logout unless lingering is on. `install` prints the `loginctl enable-linger` hint, but does not run it.

**Stop, restart and crash.**
- `stop` and `uninstall` first send `shutdown` on the socket. This request exists only on the socket; a bridge never forwards it. The daemon then marks every active loop `stopped` with reason `user`, and exits 0. After that, the command unloads the service.
- Any other end is not a user's stop: a crash, `SIGTERM` at logout or shutdown, or `daemon restart` (`launchctl kickstart -k`, `systemctl --user restart`). The loops stay as they are in the records, and the next daemon resumes them.
- `daemon restart` resumes loops on purpose. An upgrade should not drop the user's loops.
- launchd or systemd restarts the daemon after a crash. A clean exit (0) is not restarted.

**A single instance.**
- The daemon holds an exclusive `flock` on `daemon.lock` while it runs. The kernel releases it when the process dies, so a crash leaves no stale lock.
- A second daemon that cannot take the lock exits 0, and is not restarted.
- After it has the lock, the daemon removes a leftover `daemon.sock` and binds a new one.

**Files.**
- A new flag, `--daemon-dir DIR`. The default is `$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`: the parent of the existing `tasks/` directory.
- It holds `daemon.sock`, `daemon.lock` and `daemon.log`. The directory has mode 0700 and the socket 0600.
- The daemon checks the peer's uid on every connection with tokio's `UnixStream::peer_cred()`, and closes a connection from another uid.
- A Unix socket path is limited to about 104 bytes on macOS. The daemon refuses a longer one with a clear message.

**Finding tmux.** The daemon uses the absolute tmux path that `install` wrote, else the bridge's `find_tmux`. It talks to the server named by `--socket`, as the bridge does.

**When tmux is not running.**
- The daemon never starts a tmux server. It runs `new-session` only for a Task launch that a client asked for.
- Without a server it still answers record requests. Loops wait: no agent state means no action (invariant 6). It looks for a server again every 10 seconds.
- When the server ends while the daemon is attached, the daemon drops its client and looks again later.

**Its tmux client.**
- The daemon reuses `muxa-tmux`'s control client, attached with `-f ignore-size,no-output`. `Client::attach` (`crates/muxa-tmux/src/client.rs`) gets a flags parameter for this; the bridge keeps `ignore-size` alone.
- `no-output` means no `%output`, so no pane bytes reach the daemon.
- It attaches only while it has work: an active loop, a launch, or a prompt wait. It detaches after 60 seconds without work. A plugin's `client-attached` hook then runs once per stretch of work, not every 2 seconds.
- When the session it is attached to is destroyed, tmux detaches it (`docs/findings/tmux-control-mode.md`). The daemon attaches again on the next tick.
- A new repro script, `scripts/repro/daemon-client.sh`, checks on every CI tmux version, 3.2a included:
  - `attach-session -f ignore-size,no-output` is accepted;
  - with `no-output`, no `%output` arrives;
  - a long-lived `ignore-size` client next to a desktop client changes no layout over several minutes;
  - detaching changes nothing.

**Logs.**
- The daemon writes `daemon.log` itself. It rotates the file at 1 MiB and keeps one old file.
- launchd's `StandardErrorPath` (`daemon.err`) and the systemd journal catch only start-up failures.
- A line carries ids, paths, states, exit codes and durations. It never carries a title, goal, note, check command, check output, env value or prompt text (invariant 5). The default level is `info`.

### 2. One writer, one code path

- **The daemon is the usual writer of task records.** A bridge sends these record requests to the daemon when it runs: `list-tasks`, `create-task`, `update-task`, `restart-agent`, `start-loop` and `stop-loop`.
- **The socket speaks protocol v1.** It uses the same `Request` and `Message` types, as JSON lines. The daemon sends `hello` first, with its version and capabilities. It answers the requests above, plus `shutdown` (section 1). It answers `unsupported` to every other request.
- **One connection per request.**
  1. The bridge connects to the socket, with a 200 ms timeout.
  2. It reads `hello` and sends the client's request with the same `id`.
  3. It relays the daemon's `task-saved`, `task-list` and `result` to its client, then closes the connection.
- **Fallback.** When the connect fails, the bridge runs the same handlers in its own process, with direct writes and `rev`, as today. One connection per request makes this exact: each request goes to whatever runs at that moment.
- **Version.** The bridge uses the daemon only when the daemon's version is the bridge's own. Otherwise it falls back and logs a warning. This keeps two versions of the handlers from serving one client.
- **Loop requests have no fallback.** Without a usable daemon, `start-loop`, and `create-task` with `loop`, are `unsupported`. The message says which case it is:
  - "The Task Loop needs the muxa daemon. On the host, run: muxa-bridge daemon install"
  - "The muxa daemon on the host runs version X. Run: muxa-bridge daemon restart"
- **Capability.** The bridge checks the daemon once, when it sends its own `hello`, to decide on `task-loop`. If the daemon goes away later, loop requests get the message above.
- **A daemon that stops mid-request** is an `internal` error: "the muxa daemon stopped; try again". The bridge does not retry locally, because the daemon may already have written.
- **How the two paths stay one.**
  1. All record requests go through one function, `task_records::handle(env, request) -> messages`. The bridge's fallback calls it, and so does the daemon's socket server. The only branch is in the bridge: daemon or local.
  2. The `tests/task_records.rs` suite runs twice: once with a daemon on a temp socket, once without. The same assertions hold for both.
- **Correctness still rests on `rev` and the lock.** "Single writer" is a routing rule, not a promise.
  - A bridge that falls back while a daemon starts writes safely.
  - When a loop write and a user's `update-task` race, one gets `conflict`. The loop retries, as `handoff::write` does today.
- **Clients learn of record changes from a poll, not a push.**
  - Every bridge looks at the `tasks/` directory every 2 seconds, like `states.rs`. It compares each `*.json` file's name, size and mtime with the last look.
  - On a change it sends `tasks-changed`, a new message with no fields, behind the new capability `tasks-changed`. Clients list the Tasks again.
  - This covers every writer: the daemon, a fallback bridge, and `muxa-cli` on the laptop. The daemon needs no list of connected bridges.

### Prompt waits are durable

`create-task` and `restart-agent` wait up to 30 s to type their prompt (ADR 0015, 0018). The daemon owns that wait, and it is written down so that a crash does not lose it.

- The record gets `pendingPrompt {kind, since, run}`, where `kind` is `start` or `restart`. The handler writes it with the `task-saved` that ends the request, and clears it when the prompt is typed or given up.
- A daemon that starts finds every `pendingPrompt`:
  - younger than 30 s: it resumes the wait;
  - older: it clears the field, and writes the "not sent" note of ADR 0015.
- A fallback bridge writes the same field, and waits in its own `Pending`, as today. A daemon that starts later picks up a wait that the bridge left behind.
- Why not keep the waits in the bridge: they would end with the bridge, and the 100 s limit of `Pending::finish` would stay. The record is already the one place that survives every process.

### 3. The loop

A Task can have one optional loop:

| Field | Meaning |
|---|---|
| `check` | A shell command, run with `/bin/sh -c` in the Task's `dir`. |
| `maxAttempts` | Failed checks before the loop stops. 1 to 10, default 3. |
| `timeout` | Seconds one check may run. 10 to 3600, default 600. |

Only a Task inside its workspace can have a loop. That is a repo Task whose repo is in the workspace's `repos`, or a Task without a repo, whose directory is under the workspace root. A repo Task outside `repos` gets `bad-request`.

**A finished turn.** The agent has finished a turn when its state is `idle` or `done` and its `since` is at or after `armedAt`. A state with no `since` is never a finished turn (ADR 0018). tmux-agents reports a finished turn as `done` while the user is away (Context).
- `Prompt::ready` (`task_records/handoff.rs`) follows the same rule. It accepts `done` as well as `idle`, and only when `since` is at or after the time the prompt's wait began. This also fixes the start and restart prompts for tmux-agents panes while the user is away.

**When the check runs.**
1. The loop is `armed`. `armedAt` is the time the loop started, or the time it last typed a prompt.
2. On each 2-second tick, the daemon reads the Task's agent pane with `handoff::agent_pane`. One tick lists the processes once (`Processes::list`) for all loops, not once per Task.
3. `step()` ignores a state older than the `startedAt` of the Task's newest run.
4. The check runs on a finished turn that is still there 5 seconds later.

**How the check runs.**
- It is a subprocess of the daemon, in its own process group. It is never typed into a pane.
- Its environment:
  1. the tmux server's global environment (`show-environment -g`), which is what the panes get;
  2. then the Task workspace's `agent.env` (ADR 0016) on top, so the check has the same accounts as the agent;
  3. when tmux cannot be reached, the daemon's own environment with the `PATH` that `install` copied.
- stdout and stderr go to `.muxa/checks/<attempt>.log` in the Task's `dir`, at most 8 MiB. Past that, the daemon drops the rest and ends the file with one line that says so. The newest 20 logs are kept.
- After each check, the daemon writes `.muxa/CHECK.md`. It holds the attempt, the exit code, the duration, and the last 200 lines of the log in a code block, with terminal escapes removed. The Reader lists it next to `TASK.md` and `HANDOFF.md`.
- At `timeout`, the daemon kills the process group.
- `TASK.md`'s header gets a `## Check` section with the command and this text: "muxa runs this after each of your turns. The Task is ready for review when it exits 0." The agent can run it itself.

**What happens after a check.**

| Result | The loop |
|---|---|
| Exit 0 | State `passed`. A Task with status `open` gets status `review`. Note: "The check passed on attempt n." The loop ends. |
| Non-zero, attempts left | Types the fail prompt, then `armed` again. |
| Non-zero, limit reached | `stopped`, reason `limit`. |
| Timed out | `stopped`, reason `timeout`. |
| The agent was not in a finished turn when the check ended | The result is dropped and the attempt does not count. The log stays. `armed` again. |

- **Status on a pass.** The loop changes the status only from `open` to `review`. It never sets `done`, and never changes any other status. This amends ADR 0015, where only the user sets the status. `PROTOCOL.md` ("Task records", item 2) says so in P3.

The fail prompt is fixed text. Only the attempt number changes:

```
The check failed. Its output is in .muxa/checks/<n>.log. Fix the cause, then end your turn.
```

**How a prompt is typed.** This applies to the fail prompt, and to the start and restart prompts that the daemon types.
1. The agent must be in a finished turn.
2. No person may be typing. Before typing, the daemon runs `list-clients -F '#{client_activity} #{client_session} #{client_flags}'`. When a client that is not a control client, on the Task's session, was active in the last 5 seconds, the daemon waits and looks again on the next tick.
3. It types with `send-keys -l`, then Enter, as `handoff::send_prompt` does.
4. Only after the keys are sent does it write the record: the new `armedAt`, which is the time taken just before typing, and the attempt. The order is type, then write. A crash between the two leaves an older `armedAt`. The agent's next finished turn is newer than both, so the loop goes on.
5. When no moment fits within 30 seconds, the loop is `stopped` with reason `not-sent`.

`scripts/repro/client-activity.sh` checks what step 2 relies on:
- a desktop client's key press updates `#{client_activity}`;
- the daemon's own `send-keys` from a control client does not update another client's activity;
- `#{client_flags}` names control clients.

**Paused while the agent waits for the user.**
- When the agent is `waiting` (a permission, a question or a plan), the loop is `paused` with reason `waiting`. It types nothing, and runs no check. The Task is in Needs attention.
- The loop never answers the menu (ADR 0009). It sends no `1`, `y`, `Esc` or Enter, and never uses an approval card's keys.
- When the user has answered and the agent leaves `waiting`, the loop is `armed` again, with `armedAt` set to the time it paused. The turn that the user's answer lets finish is a finished turn, so the check then runs. The attempt count does not change.

**Stopped, for the user.** On any of these, the loop stops at once and does nothing more in the pane:

| Reason | When |
|---|---|
| `limit` | `maxAttempts` failed checks. |
| `timeout` | A check ran longer than `timeout`. |
| `error` | The agent's state is `error`. |
| `no-agent` | No linked session, no live agent pane, or `shellForeground: true`. |
| `not-sent` | A prompt could not be typed within 30 seconds. |
| `stalled` | The loop is `armed`, and for more than 60 seconds the agent shows a finished turn older than `armedAt`. The typed prompt did not reach it. |
| `status` | The user set the Task's status to anything but `open`. |
| `user` | `stop-loop`, `daemon stop` or `daemon uninstall`. |

- The note says why, in one sentence, with no check output.
- Every reason except `user` and `status` puts the Task in **Needs attention** (section 5).
- `start-loop` starts the loop again. The attempt count goes on from where it stopped, unless the request resets it.

**`restart-agent` and the loop.** A restart starts a new run. The restart's prompt is typed as above. The loop is then `armed` with `armedAt` set to the time of that prompt, and its attempt count does not change.

**What the loop never does.**
- It never answers a menu.
- It never restarts the agent. `restart-agent` stays a user action.
- It never pushes, merges, closes a session or removes a worktree.
- It never reads the pane's screen. All state comes from adapters (invariant 6).

**Daemon restart.** Loops are in the records, so they survive a crash or a restart. An `armed` or `paused` loop goes on. A `checking` loop goes back to `armed`, and the attempt does not count. On a clean exit the daemon kills the check's process group first. After a crash the check may still run, but its result is never read.

### 4. Who may set a check command

- **Plainly: setting a check means running a command on the host as the user.** Any client that holds the SSH key can do it, with `create-task` or `start-loop`.
- This grants nothing new. The key can already type into any pane, and a shell pane runs any command (ADR 0005, 0015). The bridge already starts agents that run commands.
- What is new is the path: a command runs with no pane to show it. So the host decides, per workspace, whether this path exists. It does so in `workspaces.toml`, which clients cannot write:

  ```toml
  [workspace.loop]
  checks = ["make check", "cargo test --workspace"]   # or checks = "any"
  ```

  - No `[workspace.loop]` table: the workspace has no loops. `start-loop`, and `create-task` with a loop, are `bad-request`. The message names the table.
  - A list: the check must be one of the strings, exactly.
  - `"any"`: any command, at most 4096 bytes, with no control character.
- `list-tasks` sends each workspace's allowed checks, so the apps can offer a picker instead of a text field.
- Installing the daemon is also the host's opt-in. Without a daemon there are no loops.
- The check command is stored in the record. It is never logged, and never placed on a tmux command line.

### 5. Wire, CLI and apps

**Protocol** (compatible within v1):
- Capability `task-loop`: the bridge reached a daemon of its own version when it sent `hello`, so loops work.
- Capability `tasks-changed` and the message `tasks-changed`, with no fields (section 2).
- `create-task` gets an optional `loop {check, maxAttempts?, timeout?}`.
- Request `start-loop {id, task, loop {check, maxAttempts?, timeout?}, reset?}`. A `task-saved` comes before the `result`.
- Request `stop-loop {id, task}`. A `task-saved` comes before the `result`.
- Record field `loop`, absent when the Task has none: `{check, maxAttempts, timeout, state, attempts, armedAt, since, reason?, lastCheck? {attempt, exit?, seconds, at}}`.
  - `state` is `armed`, `checking`, `paused`, `passed` or `stopped`. Treat an unknown state as one with no known meaning.
  - `reason` is set when the state is `paused` or `stopped` (section 3). Treat an unknown reason the same way.
  - `exit` is absent when the check timed out.
- Record field `pendingPrompt {kind, since, run}` (section 2).
- `task-list`'s `workspaces[]` get an optional `checks`: a list of strings, or `"any"`. Absent means no loops in that workspace.
- The record stays `version` 1. The new fields are additive, and older bridges keep them on rewrite (ADR 0015).
- `PROTOCOL.md` changes in these places:
  - "Task records", item 2: the loop may move `open` to `review`;
  - new sections "Task Loop" and `tasks-changed`;
  - "Documents", item 2: `.muxa/CHECK.md` joins `.muxa/TASK.md` and `.muxa/HANDOFF.md`.
- `PROTOCOL.md`, `protocol/fixtures`, and the Rust, Swift and Kotlin types change together.

**CLI.**
- `muxa-cli task new ... --check CMD [--max-attempts N] [--check-timeout S]`
- `muxa-cli task loop ID --check CMD [...] [--reset]`
- `muxa-cli task stop-loop ID`, and `muxa-cli task stop-loop --all`
- `muxa-cli tasks` shows the loop's state and attempts. `--json` carries the record as is.

**Apps (iOS and Android).**
- Task detail: a Loop row with the state, "attempt n of max", the last check's result and age, and the reason when paused or stopped. Buttons: Stop loop, Start loop, Open check log. Open check log opens `.muxa/CHECK.md` in the Reader.
- New Task sheet: "Run a check after each turn", with a picker from the workspace's `checks`, or a text field for `"any"`.
- Without `task-loop`, the controls are hidden. A line says the host needs the muxa daemon.
- On `tasks-changed`, the app lists the Tasks again.

**The Inbox rule** gets one row before the others. **Needs attention** also holds a Task whose loop is:
- `paused`, or
- `stopped` with a reason other than `user` or `status`.

The rule has three copies: Swift, Kotlin and `muxa-cli`. This change adds the shared fixture that ADR 0017 left as a follow-up: `protocol/fixtures/inbox/`, with inputs and expected groups, decoded by all three test suites.

### 6. What the daemon enables later

Not built now. Each gets its own ADR when it is needed.
- **Runs while no client is connected.** The daemon can record an agent run's start and end for every Task, and close the gaps of ADR 0018.
- **A scheduler.** Start a Task at a time, or start the next Task in a workflow when one passes (vision phase 4).
- **Push while the app is closed.** The daemon sees every state change. It could send a notice to a relay that holds the APNs and FCM credentials (ADR 0008). That is outbound only; the daemon still listens on no port.

Out of scope:
- Any network listener, web UI or remote API.
- Approving, denying or answering anything for the agent. Setting a Task to `done`. Pushing, merging or closing.
- Restarting an agent automatically.
- Running checks in a sandbox or a container.
- A Mac with no GUI login, several hosts, and Windows.

### 7. Testing

- **No launchd or systemd in tests.**
  - Unit tests compare `install --print` with golden plist and unit files.
  - The `launchctl` and `systemctl` calls sit behind a small trait. Unit tests use a fake.
  - Loading the real LaunchAgent is a manual check by the developer, like `make install-ipad`.
- **`make test-tmux` runs a real daemon.**
  - A test starts `muxa-bridge daemon run --socket muxa-test-<name> --state-dir <tmp>/state --workspaces <tmp>/workspaces.toml --daemon-dir <short tmp>` as a child process.
  - The daemon dir is short and under `/tmp`, because of the socket path limit.
  - The test sends `shutdown` at the end, or kills the daemon to test a crash.
  - No test touches the default server (invariant 1).
- **The record suite runs on both paths**, with and without a daemon (section 2).
- **A fake agent**: a shell script in the agent pane, in the style of the sandbox's `scripts/lib/fake-agent.sh`, which sets no state today.
  - It reads a line, sets `@agent_state running` and `@agent_state_at` on its pane, and writes a marker file.
  - It then ends the turn with `done`, as tmux-agents does while the user is away, or with `idle`, chosen per test.
  - It can also set `waiting` or `error` on request.
- **Fake checks:**
  1. `test -f fixed`: fails once, the fake agent writes `fixed`, then it passes. The Task becomes `review`, after 2 attempts. This runs once with `done` and once with `idle`.
  2. `false`: stops with `limit` after `maxAttempts`. The fail prompt was typed `maxAttempts - 1` times.
  3. `sleep 60` with `timeout 10`: stops with `timeout`, and the process group is gone.
  4. The fake agent goes `waiting`: the loop is `paused`, and nothing is typed. The test then answers the fake agent itself. The agent ends its turn, the check runs, and the loop goes on.
  5. The daemon is killed during a check and started again: the loop is `armed`, and the attempt did not count.
  6. `daemon stop`: every loop is `stopped` with reason `user`, and a new daemon does not resume them.
  7. A daemon killed during a start-prompt wait, and started again within 30 s: the prompt is typed once.
  8. A desktop client on the Task's session types within 5 s of a fail: the prompt waits until the client is quiet.
- Every loop test records the layout with `scripts/layout-snapshot.sh` before and after. The diff is empty (invariant 2).
- A test greps `daemon.log` for the goal, the check command and a marker in the check output. None appears (invariant 5).
- Socket permissions (0700 dir, 0600 socket) and the single instance (a second daemon exits 0) are tested. The peer uid check is a unit test.
- The loop's decisions are a pure function, `step(state, observation) -> (state, action)`. It is unit-tested without tmux, including `done`, a stale `since`, `stalled`, and a state older than the newest run.

## Consequences

- A Task can fix itself until a check passes, while the phone is off. It stops or pauses for the user at every gate.
- The host runs a resident process, and the user installs it by hand. Without it, muxa works as today, with no loops.
- After an upgrade, loops stop working until `daemon restart`. The `unsupported` message and `daemon status` say so.
- The record format gains `loop` and `pendingPrompt`. Older bridges keep them, but cannot show them.
- The Task status is no longer only the user's: the loop moves `open` to `review`.
- The Inbox rule changes in three clients at once, now pinned by a shared fixture.
- A check runs with the user's rights and the workspace's tokens. The workspace's `checks` list is the host's control.
- Rejected: **the daemon inside tmux** (a hidden session or window). It would show in session lists and on the desktop, and end with `kill-server`. It could not notice that tmux is gone. Its environment would be the tmux server's, and nothing would restart it.
- Rejected: **agent hooks**, for example a Claude Code `Stop` hook that runs the check.
  - They are per agent, and Codex has no equal.
  - They live in config directories that muxa would have to edit, one per workspace (ADR 0016).
  - The agent's own process would run the loop, but muxa owns the loop (vision).
  - A hook cannot stop and escalate across agent restarts.
  - Hooks can still be a faster *signal* of a turn's end later.
- Rejected: **no daemon**, with the loop in the per-connection bridge. It ends when iOS closes the connection, which is when a loop is useful. Two bridges would run two loops and type twice.
- Rejected: **a separate `muxa-daemon` binary.** See section 1.
- Rejected: **the daemon as the only writer**, with no fallback. Without the daemon, Tasks would stop working, and the store of ADR 0015 already makes several writers safe.
- Rejected: **a persistent bridge-to-daemon connection.** The fallback would follow the state at connect time, not at the request.
- Rejected: **a push from the daemon to bridges.** It misses fallback writes and `muxa-cli` on the laptop. The poll sees every writer.
- Rejected: **a long-lived control client with output on.** Pane bytes would reach a process that needs none of them.
- Rejected: **stopping the loop on `waiting`.** The user answers, and the loop should go on without a second tap.

## Implementation plan

Each phase is verified on its own. The phases are ordered by risk. P1 and P2 are the risky ones: a resident process, and typing into panes with no user watching. Each phase owns the files listed for it, so that parallel agents do not collide. When a phase must touch another phase's file, it waits for that phase to merge.

| Phase | What | Owns | Verified by | Depends on |
|---|---|---|---|---|
| P0. `done` is a finished turn | `Prompt::ready` accepts a fresh `done` (B1). A bug fix today, for the start and restart prompts. | `task_records/handoff.rs`, its tests, `tests/task_records.rs` (a `done` case) | `make check`, `make test-tmux` | none |
| P1a. Lifecycle and forwarding | `daemon run/install/stop/restart/uninstall/status`, `--daemon-dir`, the lock, the socket and its modes, the peer uid, `hello`, `shutdown`, the version check. `task_records::handle`. The per-request forward and the fallback, for `list-tasks` first. | new `src/daemon/` (`mod.rs`, `install.rs`, `socket.rs`, `lock.rs`, `log.rs`), `args.rs`, `main.rs`, new `task_records/remote.rs`, the record-request branch in `server.rs`, new `tests/daemon.rs`, `tests/common/daemon.rs` | `make check`; `make test-tmux`; golden files for `install --print`; the developer loads the LaunchAgent once | P0 |
| P1b. Writes and durable waits | The forward for `create-task`, `update-task` and `restart-agent`. `pendingPrompt` and its resume. The `tasks/` poll and `tasks-changed` in the bridge. The record suite on both paths. | `task_records/create.rs`, `restart.rs`, `mod.rs`, `records/mod.rs`, `records/store.rs`, new `src/task_watch.rs`, `tests/task_records.rs` | `make test-tmux` (both paths, crash during a wait) | P1a |
| P1c. The daemon's tmux client | The flags parameter of `Client::attach`, the lazy `ignore-size,no-output` client, and reattach. Two repro scripts and their findings rows. | `crates/muxa-tmux/src/client.rs`, new `src/daemon/tmux.rs`, `scripts/repro/daemon-client.sh`, `scripts/repro/client-activity.sh`, `docs/findings/tmux-control-mode.md` | `make repro` on 3.2a, 3.4 and the latest; `make test-tmux` | P1a |
| P2. Loop engine | `step()` and its unit tests. The check runner: process group, timeout, environment, log cap, `CHECK.md`. The typing guard (finished turn, client activity, type then write). `## Check` in `TASK.md`. `[workspace.loop]`, and the workspace `repos` rule. The tick, with one process listing. | new `src/loops/` (`mod.rs`, `step.rs`, `check.rs`, `typing.rs`), new `records/loops.rs`, `records/workspaces.rs`, `task_records/brief.rs`, `docs/files.rs`, new `tests/loops.rs` | `make check`; `make test-tmux` with the fake agent and the eight fake checks; the layout snapshot; the log grep | `step()` and the runner: none, so they can start with P1. The tick: P1b and P1c. |
| P3. Protocol and CLI | `PROTOCOL.md`, the fixtures including `inbox/`, the Rust types, `start-loop`, `stop-loop`, `loop` on `create-task`, `checks` on workspaces, `tasks-changed`, `pendingPrompt`. The `muxa-cli` flags, `stop-loop --all` and the output. Swift and Kotlin decoding. | `crates/muxa-protocol/`, `protocol/`, `crates/muxa-bridge/src/convert.rs`, `crates/muxa-cli/`, `apple/MuxaKit/Sources/MuxaProtocol/`, `android/protocol/` | `make check`, `make test-apple`, `make test-android` | This ADR accepted. Parallel with P1 and P2. |
| P4. iOS UI | The Loop row, the New Task option, the Inbox row, Open check log. The demo bridge gets loop data. | `apple/Muxa/`, `apple/MuxaKit/Sources/MuxaSession/`, `apple/MuxaKit/Sources/MuxaUI/` | `make test-app`, `make test-app-iphone` | P3 fixtures. Parallel with P5. |
| P5. Android UI | The same as P4. | `android/app/`, `android/demo/`, `android/session/` | `make test-android-app`, `make lint-android` | P3 fixtures. Parallel with P4. |
| P6. Docs | `docs/plan.md`, `SECURITY.md` (the daemon, the socket, check commands), the README install steps, `docs/agents.md`. ADR 0019 to Accepted. | those files | Review | P1 to P5 |
