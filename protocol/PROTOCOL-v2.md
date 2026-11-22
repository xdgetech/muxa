# muxa protocol v2

Status: draft (2026-11-21), built in slices B1–B3 (resume: B3) of `docs/plan.md`. ADR 0067. Since F5 (ADR 0071) it is muxa's only protocol.

## Transport

- A client on another device runs `muxa-bridge connect` over SSH exec. The bridge relays stdin and stdout to the server's socket and adds nothing.
- A device key whose `authorized_keys` line forces `command="…/muxa-bridge"` still reaches v2: sshd runs `muxa-bridge` with the client's command in `SSH_ORIGINAL_COMMAND`, and when its last word is `connect` the bridge relays as `muxa-bridge connect` does. The key can run nothing else.
- A client on the host connects to the server's Unix socket directly: `server.sock` in the daemon's directory (`$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`), beside v1's `daemon.sock`.
- The socket is mode 0600 in a 0700 directory; the server checks the peer's uid.

## Frames

Every byte in both directions is a frame:

```
[channel u32 LE][kind u8][len u32 LE][payload: len bytes]
```

A payload is at most 16 MiB. An unknown kind or an oversized frame ends the connection.

| Kind | Name | Payload |
|---|---|---|
| 0 | DATA | channel 0: one JSON control message. Terminal channel: see below |
| 1 | CREDIT | u32 LE: output bytes the client consumed |
| 2 | CLOSE | the channel is over. From the server: a JSON exit `{"reason","exit_code","signal"}` |
| 3 | RESIZE | client → server: u16 LE cols, u16 LE rows |
| 4 | ACK | server → client: u64 LE, the input sequence number just applied |

## Terminal channels

The server opens one channel per attach. Channel numbers are the server's.

- **Output** (server → client DATA): `[offset u64 LE][bytes]`. `offset` is the position of the first byte in the stream since its last paint. A paint starts at offset 0.
- **Paint:** before a paint, the server sends the control message `paint` with the channel. The client then resets its terminal emulator. A paint follows an attach, and a resume the server's buffer cannot cover.
- **Input** (client → server DATA): `[seq u64 LE][bytes]`. `seq` starts at 1 per channel and grows by 1. The server applies each `seq` once and answers ACK. A repeated `seq` (after a resume) is acknowledged again, not applied again.
- **Credit:** each channel starts with 256 KiB of output credit. The client sends CREDIT for the output bytes (not headers) it has consumed. With no credit left, the server stops reading the terminal for this client, and the backend's own flow control slows the program.
- **Resize** (client → server RESIZE): the terminal takes this size while this client is its last active viewer. The program redraws itself; no paint is sent.
- **Close:** the client sends CLOSE to detach. The server sends CLOSE with the exit when the program ends or the terminal goes away.

## Control messages (channel 0)

JSON objects with `type`. A request has an `id` (u64, chosen by the client). The server answers `{"type":"reply","id",...}` or `{"type":"error","id","code","message"}`. Messages without `id` are notifications.

### Handshake

The client's first frame is `hello`. The server answers `welcome`, or `error` and closes.

```json
{"type":"hello","version":2,"client":"muxa-ios 1.3","resume":null}
{"type":"welcome","version":2,"server":"muxa 0.1.0","backend":"herdr-core 0.1.21","token":"…","resumed":[]}
```

`token` identifies the client's session for a resume.

### Resume

A client that lost its connection sends `hello` with `resume`: its token and, per open channel, where it stopped.

```json
{"type":"hello","version":2,"client":"muxa-ios 1.3","resume":{"token":"…","channels":[
  {"channel":1,"terminal":"t7","offset":20480,"input_seq":12,"cols":120,"rows":40,"history":0}]}}
{"type":"welcome",…,"token":"…","resumed":[{"channel":1,"input_seq":13}]}
```

- `offset` is the next output offset the client expects. `input_seq` is the last input it got an ACK for.
- When a client goes away, the server keeps its attachments for 10 minutes. Each keeps reading its terminal into a buffer of the last 256 KiB, in memory only.
- `resumed` lists the channels that go on, with the same numbers. A channel not listed is over (its terminal is gone, or the request was not valid). The server never gives a named channel number to a new attach.
- If the buffer still holds `offset`, output continues from `offset` with no paint. Otherwise the server attaches the terminal again at `cols`×`rows` and sends a paint.
- `resumed[].input_seq` is the last input the server applied. The client resends its unacknowledged input after it; the server acknowledges a repeated `seq` without applying it again. `null` means the server cannot tell (its state for the channel is gone, for example after a restart). The client then drops its unacknowledged input instead of sending it again, and the server applies only `seq` above the client's `input_seq`.
- Credit starts again at 256 KiB on each resumed channel.
- `token` in `welcome` is the same when the server found the session, and a new one otherwise.
- If a connection with the same token is still open (the client changed networks before the server noticed), the server closes it and takes over its attachments.
- A program that ended while the client was away: the client gets the output it missed, then CLOSE with the exit.

### Requests

| Request | Reply |
|---|---|
| `{"type":"terminals"}` | `{"terminals":[Terminal...]}`: the live terminals |
| `{"type":"attach","terminal","cols","rows","history"}` | `{"channel"}`. A `paint` notification and the first output follow |
| `{"type":"detach","channel"}` | `{}`. Same as CLOSE on the channel |
| `{"type":"start-task","workspace","title","goal"?,"places"?,"connections"?,"mcp"?,"plan"?,"loop"?}` | `{"terminal":Terminal,"existing"}`: the Task's agent terminal. `loop` (LoopSpec) also starts its Task Loop |
| `{"type":"watch"}` | `{}`. `changed`, `removed`, `resync` and `inbox` notifications follow |
| `{"type":"inbox"}` | `{"inbox":Inbox}` |
| `{"type":"parse-capture","text","today"}` | `{"capture":{"kind","title","workspace","due"}}`. `today` is the device's date, `YYYY-MM-DD` |
| `{"type":"inbox-add","kind","title","note"?,"workspace"?,"due"?}` | `{"inbox":Inbox}`. The item goes on top |
| `{"type":"inbox-move","item","index"}` | `{"inbox":Inbox}`. An index past the end means last |
| `{"type":"inbox-later","item","until"}` | `{"inbox":Inbox}`. `until` is Unix seconds, in the future |
| `{"type":"inbox-done","item"}`, `inbox-reopen`, `inbox-delete` | `{"inbox":Inbox}` |
| `{"type":"routines"}` | `{"routines":[Routine...],"problems":[{"workspace","reason"}...]}` |
| `{"type":"run-routine","workspace","name"}` | `{}`. The run reports through the Inbox; `conflict` while it already runs |
| `{"type":"task-review","terminal","note"?}` | `{"inbox":Inbox}`. One open Review item for the Task of this agent terminal; `bad-request` for a terminal that is not a Task's agent |
| `{"type":"reader-list","terminal","kind","html"?}` | `{"list":ReaderList}`: the files of the terminal's folder. See Reader |
| `{"type":"reader-open","terminal","kind","path","html"?,"scope"?}` | `{"reader","content":ReaderContent}`. `reader` and `reader-ended` notifications follow |
| `{"type":"reader-close","reader"}` | `{}`. `not-found` after `reader-ended` |
| `{"type":"chat-open","terminal"}` | `{"chat","update":ChatUpdate}`: the conversation of a Task's agent. `chat` and `chat-ended` notifications follow |
| `{"type":"chat-close","chat"}` | `{}`. `not-found` after `chat-ended` |
| `{"type":"loops"}` | `{"loops":[Loop...]}`: the Task Loops of the live Task agents |
| `{"type":"loop-start","terminal","loop":LoopSpec,"reset"?}` | `{"loop":Loop}`. Starts or restarts the loop of a Task's agent |
| `{"type":"loop-stop","terminal"}` | `{"loop":Loop}`. Stopped with reason `user` |

`Terminal`: `{"id","name","live","status":{"state","app"}|null,"labels":{},"cols","rows"}`. It never carries screen text. `status` is what the program last reported with OSC 7501 (`state` such as `working`, `blocked`, `done`); `null` when it reported nothing. The server never guesses one.

`start-task` makes a Task's places and runs the Workspace's agent in the working place, in plan mode when `plan` is true (ADR 0068). Each place is one of:

- `{"kind":"folder"}`: a new folder `<root>/tasks/<slug>-<id>`;
- `{"kind":"branch","repo","branch","base"?}`: a worktree of the repo on the branch, made or found;
- `{"kind":"read","repo"}`: the repo's main checkout, never written.

A `repo` is absolute, or relative to the Workspace's root. The working place is the folder, else the first branch; with neither, the server adds a folder, so no `places` means one folder. A Task has at most one folder and one place per repo. The working place gets `.muxa/TASK.md` with the title, the goal and every place with its mode. A restarted Task keeps its own. Claude Code also gets the other places as additional directories, with edits denied in the read-only ones (`--settings .muxa/claude-settings.json`). Read only is a rule for the agent, not a boundary.

If the Task's agent is still running in that working place, the reply has it with `existing: true` and nothing new starts. A folder is new each time, so a folder Task always starts a new agent. The agent terminal's `name` is the title, and it has these labels:

- `muxa.kind` = `task`, `muxa.role` = `agent` and `muxa.dir`, the working place;
- `muxa.workspace`;
- `muxa.repo` (the main checkout) and `muxa.branch`, when the working place is a branch;
- `muxa.places`: a JSON list of `{"kind","path","branch"?}`, the working place first.

`connections` names the Workspace's connections the agent gets as environment variables, besides the ones on by default (ADR 0069). Their secrets come from the host's Keychain and never cross the wire. A missing or locked secret fails the start before any place is made.

`mcp` names the Workspace's MCP servers the agent gets, besides the ones on by default (ADR 0069). Claude Code gets only those, in `.muxa/mcp.json` with `--strict-mcp-config`; with none, the file is empty, so no other configured server loads. The connections a server needs go into the agent's environment, never into the file. Only Claude Code takes MCP servers: for any other agent, a server named or on by default is `bad-request`, before any place is made.

An unknown Workspace, connection or MCP server, a missing title, a wrong repo or branch, or a repo named twice is `bad-request`.

### Notifications

| Notification | Meaning |
|---|---|
| `{"type":"paint","channel"}` | The next output on the channel starts a fresh screen at offset 0 |
| `{"type":"changed","terminal":Terminal}` | After `watch`: a terminal started, or its status, size or liveness changed. It carries the whole terminal as it is now |
| `{"type":"removed","terminal":"<id>"}` | After `watch`: the terminal is gone |
| `{"type":"resync"}` | After `watch`: changes were lost. The client sends `terminals` again |
| `{"type":"loops","loops":[Loop...]}` | After `watch`: a Task Loop changed. It carries every loop |
| `{"type":"chat","chat","update":ChatUpdate}` | A followed conversation changed |
| `{"type":"chat-ended","chat","reason"}` | A followed conversation ended, e.g. the agent exited. The chat is closed |
| `{"type":"reader","reader","content":ReaderContent}` | An open Reader file changed. It carries the whole file as it is now |
| `{"type":"reader-ended","reader","reason"}` | An open Reader file is gone, no longer listed, or cannot be read. The reader is closed |

A watch ends with the connection, and so do open Reader files. After a resume the client sends `watch` and `inbox` again, and opens its Reader files again.

### Task Loop

ADR 0019, on herdr-core (plan F3). After each finished turn of a Task's agent (`done` or `idle` through OSC 7501), the host runs the loop's check in the working place. A pass makes one Review item in the Inbox and ends the loop. A fail types a fixed prompt, `The check failed. Its output is in .muxa/checks/<n>.log. Fix the cause, then end your turn.`, until `max_attempts` failed checks; the last one stops the loop and makes a Failed item. While the agent waits for the user (`blocked`, `waiting`), the loop is paused; it goes on when the agent works again. The loop never answers the agent's menus. It lives in the working place's `.muxa/loop.json`, so a restarted daemon goes on with it.

`LoopSpec`: `{"check"?,"max_attempts"?,"timeout"?}`.
- `check` is a shell command run in the working place. The Workspace's `[workspace.loop] checks` must allow it: one of the listed commands exactly, or any command of at most 4096 bytes with no control character for `"any"`. Without `check`, the Workspace's only listed command.
- `max_attempts` is 1 to 10, default 3. `timeout` is the seconds one check may run: 10 to 3600, default 600.
- A Workspace without `checks`, or a check it does not allow, is `bad-request`; `start-task` then starts nothing.

`Loop`: `{"terminal","check","max_attempts","timeout","state","attempts","armed_at","since","reason"?,"last_check"?}`.
- `state`: `armed` (waits for the next finished turn), `checking`, `paused`, `passed` or `stopped`.
- `attempts`: the failed checks so far; `loop-start` keeps them unless `reset`.
- `reason`, when paused or stopped: `waiting`, `limit`, `error` (the check could not run), `no-agent` (the agent ended), `not-sent` (the prompt was not typed) or `user`.
- `last_check`: `{"attempt","exit"?,"seconds","at"}`; `exit` is absent when the check timed out. Times are Unix seconds.

The check's output is in `.muxa/CHECK.md` and `.muxa/checks/<n>.log`, which the Reader lists. Neither the check command's output nor the loop file reaches the server's log.

### Chat

Plan F4. The conversation of a Task's agent, read from the agent's own records through its adapter (`docs/agents.md`): Claude Code's session whose working folder is the agent's `muxa.dir`. Another terminal, or a folder where no Claude Code runs, is `bad-request`. The server reads each open chat about every 0.7 s. A chat ends with the connection; after a resume the client opens it again.

`ChatUpdate`: `{"reset"?,"entries":[ChatEntry...],"truncated"?,"prompt"?}`.
- With `reset`, `entries` replace everything the client has; `truncated` says older entries exist. Without it, each entry replaces the one with its `id`, or is added at the end. `chat-open`'s update always has `reset`.
- `ChatEntry`: `{"id","kind","text"?,"tool"?}`, as in protocol v1: `kind` is `user`, `assistant`, `tool` or `notice`; `tool` is `{"name","summary","result"?,"error"?}`.
- `prompt`: what the agent waits on now, `{"entry","kind","question"?,"options":[{"label","input","cancel"?}]}`; `kind` is `permission`, `question` or `plan`. Every update carries the current one; absent means none is known. A client picks an option by sending its `input` as the terminal's input. The server offers only keys the adapter checked against that agent's menu; muxa never answers a menu by itself (ADR 0009).

Conversation text never reaches the server's log.

### Reader

The Reader shows the files of a terminal's folder, its `muxa.dir` label (plan F2): a Task's working place or a Routine run's folder. Another terminal has none: `bad-request`. A client names a file only by a path from `reader-list`; the server lists again on each read, so a file that leaves the list ends.

`kind` is one of:

- `docs`: the repo's Markdown files, or outside a repo every Markdown file in the folder, then `.muxa/TASK.md`, `.muxa/HANDOFF.md` and `.muxa/CHECK.md` when present. `html: true` adds HTML files (ADR 0065). Names that start with a dot are never listed, except those three.
- `changes`: the files that differ from HEAD (staged, unstaged and untracked), and in `branch` what the branch's commits changed since it left its base. Outside a repo: `bad-request`. `scope` in `reader-open` is `uncommitted` (the default) or `branch`.
- `images`: the repo's PNG, JPEG, GIF, WebP and HEIC files, ignored ones too, newest first; outside a repo, the folder's. At most 11 MiB each, so one fits in a frame.

`ReaderList`: `{"root"?,"entries":[ReaderEntry...],"truncated"?,"branch"?:{"base","commits","entries","truncated"?}}`. `root` is the repo's folder name.

`ReaderEntry`: `{"path","title"?,"mtime"?,"size"?,"status"?,"old_path"?,"added"?,"removed"?}`. Documents have `title` for HTML; images `size`; changes `status` (`modified`, `added`, `deleted`, `renamed`, `untracked`, `conflicted`), `old_path` for a rename and the line counts for a text file.

`ReaderContent`: `{"path","text"?,"data"?,"binary"?,"mtime"?}`. A document or a diff is `text`; a diff of a binary file is `binary: true`; an image is `data`, its bytes in base64.

The server reads each open file every 2 s and sends `reader` only when it changed. File content never reaches the server's log.

### Routines

`Routine`: `{"workspace","name","enabled","at"?,"timezone"?,"schedule"?,"next_due"?,"running_since"?,"last"?}`. `at` is `HH:MM` in `timezone`; `schedule` the five cron fields (absent: every day). A Routine without `at` runs only on `run-routine`. `last`: `{"at","ok","reason"?,"published"?}`, the newer of the last success and failure. Times are Unix seconds.

### Inbox

ADR 0066. `Inbox`: `{"revision","items":[Item...],"done":[Item...]}`. `items` are the open items in the user's order; `done` the last 100 done, newest first. Each change adds 1 to `revision`.

`Item`: `{"id","kind","title","note"?,"workspace"?,"due"?,"later_until"?,"created","source"}`.
- `kind`: `review`, `reply`, `decide`, `read`, `todo`, `failed`.
- `due`: `YYYY-MM-DD`. `created` and `later_until`: Unix seconds.
- An item with `later_until` in the future stays in `items` and the client hides it. At that time the server puts it back on top, clears `later_until` and sends `inbox`.
- `source`: `capture`, `task` or `routine`.

`parse-capture` reads a line by ADR 0066's rules: a first word `reply`, `read` or `decide` sets the kind (else `todo`); `#name` sets the Workspace only when one has that name; `due` takes `today`, `tomorrow`, a weekday (the next one, today included), `M/D` (the next one) or `YYYY-MM-DD`. The client shows the fields, lets the user change them, and sends `inbox-add`. A title is 1 to 300 characters; a note at most 4 KiB.

Items the server makes:
- **Review** (`source: task`): on `task-review`. An agent can send it for its own Task (`muxa-cli v2 review` uses `HC_TERMINAL_ID`). While one is open, another `task-review` for the Task makes none.
- **Failed** (`source: task`): a Task's agent ended with a non-zero code, on a signal it did not get from a kill, or was lost. Once per terminal. A clean exit or a kill makes none.
- **Read** and **Decide** (`source: routine`): a Routine's briefing and proposal entries; **Failed** for a Routine's failure. Once each. Entries from before the server first ran are not copied.

### Error codes

`bad-request`, `version`, `not-found`, `backend`, `closed`, `conflict`.

## Not yet specified

Review from a check or a draft PR, closing a Task, Task places (ADR 0068), and everything from protocol v1 beyond terminals: Tasks, Inbox, Reader, Chat, Routines. Each is added here when it is built.
