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
| `{"type":"start-task","workspace","title","goal"?,"places"?,"connections"?,"mcp"?,"plan"?,"loop"?,"bot"?}` | `{"terminal":Terminal,"existing"}`: the Task's agent terminal. `loop` (LoopSpec) also starts its Task Loop. `bot` talks to a Bot; see Talking to a Bot |
| `{"type":"task-shell","terminal"}` | `{"terminal":Terminal,"existing":false}`: a new shell in the working place of the Task whose live agent is `terminal`. See Task shells |
| `{"type":"close-terminal","terminal"}` | `{}`. Ends a Task's shell and removes its terminal; any other terminal is `bad-request` |
| `{"type":"upload","terminal","name","data"}` | `{"path"}`: a file from the device into the Task's working place, at `path` relative to it. See Uploads |
| `{"type":"close-task","terminal","worktree"?}` | `{"inbox":Inbox}`. Closes the Task of the agent `terminal` as `inbox-approve` does, without an item, and marks the Task's open items Done. See Task actions |
| `{"type":"watch"}` | `{}`. `changed`, `removed`, `resync` and `inbox` notifications follow |
| `{"type":"inbox"}` | `{"inbox":Inbox}` |
| `{"type":"parse-capture","text","today"}` | `{"capture":{"kind","title","workspace","due"}}`. `today` is the device's date, `YYYY-MM-DD` |
| `{"type":"inbox-add","kind","title","note"?,"workspace"?,"due"?}` | `{"inbox":Inbox}`. The item goes on top |
| `{"type":"inbox-move","item","index"}` | `{"inbox":Inbox}`. An index past the end means last |
| `{"type":"inbox-later","item","until"}` | `{"inbox":Inbox}`. `until` is Unix seconds, in the future |
| `{"type":"inbox-done","item"}`, `inbox-reopen`, `inbox-delete` | `{"inbox":Inbox}` |
| `{"type":"routines"}` | `{"routines":[Routine...],"problems":[{"workspace","reason"}...],"off"?:[{"workspace","count"}...]}` |
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
| `{"type":"workspaces"}` | `{"workspaces":[Workspace...]}`: what a Task may use. See Task actions |
| `{"type":"inbox-approve","item","worktree"?}` | `{"inbox":Inbox}`. Closes the Task of a Review item; `worktree` is `remove` (the default) or `force`. See Task actions |
| `{"type":"inbox-request-changes","item","comment"}` | `{"inbox":Inbox}`. Types the comment into the Task's agent and resolves the Review item |
| `{"type":"inbox-retry","item"}` | `{"inbox":Inbox}`. Runs again what made a Failed item and resolves it |
| `{"type":"inbox-apply","item"}` | `{"inbox":Inbox}`. Applies the item's `settings_proposal` and marks it Done. See Talking to a Bot |
| `{"type":"inbox-suggestion","item","accept"}` | `{"inbox":Inbox}`. Accepts or dismisses the item's `suggestion`. See Capture assistant |
| `{"type":"settings"}` | `{"settings":Settings}`: the host's editable settings. See Settings |
| `{"type":"settings-update","revision","ops":[Op...]}` | `{"settings":Settings}`: the settings after the ops. See Settings |
| `{"type":"migrate-v1","dry_run"?}` | `{"tasks":[V1Task...],"added"}`: one Inbox item per open Task of protocol v1. See Migration from v1 |

`Terminal`: `{"id","name","live","status":{"state","app"}|null,"labels":{},"cols","rows"}`. It never carries screen text. `status` is what the program last reported with OSC 7501 (`state` such as `working`, `blocked`, `done`); `null` when it reported nothing. The server never guesses one.

`start-task` makes a Task's places and runs the Workspace's agent in the working place, in plan mode when `plan` is true (ADR 0068). Each place is one of:

- `{"kind":"folder"}`: a new folder `<root>/tasks/<slug>-<id>`;
- `{"kind":"branch","repo","branch","base"?}`: a worktree of the repo on the branch, made or found;
- `{"kind":"read","repo"}`: the repo's main checkout, never written;
- `{"kind":"existing","path"}`: a folder or git worktree that is there already (ADR 0076). Nothing is made. It is the working place and comes without a folder or a branch place.
- `{"kind":"checkout","repo"}`: the repo's own main checkout as the working place, as it is (ADR 0077). Like `existing`, it comes without a folder or a branch place, nothing is made, and the same busy check and `--continue` apply. On `muxa.places` it is a `checkout` place with the branch the checkout had. The agent gets `muxa.checkout` = the `repo` as the request gave it, and `muxa.repo`; a second `start-task` with the same Workspace and `repo` returns the running agent with `existing: true`. Approve never removes it.
- `{"kind":"all-read"}`: every repo of the Workspace (the scan of its root and the listed repos), each as a `read` place. A repo already named by another place is not added twice. With no folder or branch, the server adds the folder as usual.

A `repo` is absolute, or relative to the Workspace's root. The working place is the folder, else the first branch; with neither, the server adds a folder, so no `places` means one folder. A Task has at most one folder and one place per repo. The working place gets `.muxa/TASK.md` with the title, the goal and every place with its mode. A restarted Task keeps its own. Claude Code also gets the other places as additional directories, with edits denied in the read-only ones (`--settings .muxa/claude-settings.json`). Read only is a rule for the agent, not a boundary.

An `existing` path is absolute. With its symlinks resolved, it is a folder inside the Workspace's root, or a linked worktree of one of the Workspace's repos; else `bad-request`. A worktree on a branch is a `branch` place with that branch on `muxa.places`, so Approve closes it as below; any other path is a `folder` place. `.muxa/TASK.md` is written only when it is missing, and `.muxa/` goes into the repo's `info/exclude`. When muxa's agent already runs there (`muxa.dir` is the path), the reply has it with `existing: true`. When another `claude` or `codex` process works in the folder, the request is `conflict` and nothing is written. Claude Code gets `--continue` when its config dir holds a conversation for the folder.

If the Task's agent is still running in that working place, the reply has it with `existing: true` and nothing new starts. A folder is new each time, so a folder Task always starts a new agent. The agent terminal's `name` is a slug of the title (herdr-core takes 1-32 of `a-z 0-9 - _ .`; a title with none of them gives no name), and it has these labels:

- `muxa.kind` = `task`, `muxa.role` = `agent` and `muxa.dir`, the working place;
- `muxa.workspace`;
- `muxa.title`: the title as the user wrote it. Clients show it;
- `muxa.repo` (the main checkout) and `muxa.branch`, when the working place is a branch;
- `muxa.places`: a JSON list of `{"kind","path","branch"?}`, the working place first.
- `muxa.connections`: a JSON list of every connection the agent got, when it got any. Names only.
- `muxa.mcp`: a JSON list of every MCP server the agent got, when it got any. Names only.
- `muxa.bot`: the Bot a talk Task talks with (ADR 0083). A Routine run of a Bot's Routine and a team member (`muxa.kind` = `handoff`, ADR 0085) carry it too; a member also carries `muxa.lead`, the lead it works for (ADR 0086).

Every agent gets all of its Workspace's connections as environment variables (ADR 0075). Their secrets come from the host's Keychain and never cross the wire back. A missing or locked secret fails the start before any place is made.

Every agent gets all of its Workspace's MCP servers (ADR 0075). In a Workspace that lists any, Claude Code gets them in `.muxa/mcp.json` with `--strict-mcp-config`, so no other configured server loads; a Workspace without any adds no MCP flags. The connections a server needs go into the agent's environment, never into the file. Only Claude Code takes MCP servers: for any other agent in a Workspace with servers, `start-task` is `bad-request`, before any place is made.

`connections` and `mcp` in `start-task` are deprecated and ignored (ADR 0075). The server still accepts them, and an unknown name is not an error.

An unknown Workspace, connection or MCP server, a missing title, a wrong repo or branch, or a repo named twice is `bad-request`.

### Task shells

ADR 0079. A Task's shells are terminals beside its agent, in its working place, so every client shows them under the Task.

- `task-shell` runs the host user's login shell (`$SHELL -l`, else the system's default shell) with `muxa.kind` = `task`, `muxa.role` = `shell`, the agent's `muxa.dir`, and its `muxa.workspace` and `muxa.title`. Its `name` is the agent's name with `-sh`.
- A shell gets no connections or MCP servers. Secrets go only into an agent's environment (ADR 0069).
- A Task has at most 3 live shells; a fourth is `conflict`. A terminal that is not a live Task agent is `bad-request`.
- `close-terminal` ends a shell and removes its terminal. It refuses every other terminal, so muxa never ends what it did not start this way.
- Closing a Task (`inbox-approve`, `close-task`) ends its shells with its agent.
- Clients group a shell with the agent that has the same `muxa.dir`. They list it with the Task, never as a Task or among other terminals.

### Uploads

ADR 0081. A client copies an image or a log from its device into a Task's working place, so the Task's agent can read it.

- `terminal` is the Task's agent or one of its shells; any other terminal is `bad-request`. `data` is the file's bytes in base64, at most 10 MB before encoding.
- `name` is a file name only: no `/` or `\`, not starting with `.`, no control character, at most 255 bytes. Anything else is `bad-request`.
- The host writes the file to `<working place>/.muxa/uploads/<name>`. It never replaces a file: a taken name becomes `name 2.ext`, `name 3.ext`. A `.muxa/uploads` that leads out of the working place (a symlink) is `bad-request`.
- `path` is relative to the working place, e.g. `.muxa/uploads/build.log`. The apps type it into the terminal, without Enter.
- The file's bytes never reach a log, and a malformed `upload` message is refused without quoting its values.

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

`Routine`: `{"workspace","name","enabled","at"?,"timezone"?,"schedule"?,"next_due"?,"running_since"?,"last"?,"bot"?,"on"?}`. `bot` names the Bot that owns it (ADR 0082). `on: "review"` runs it each time a Task of its Workspace asks for review (ADR 0084). `at` is `HH:MM` in `timezone`, the earliest time of the day; `schedule` the five cron fields (absent: every day). Minute and hour may be lists, ranges and steps (`*/30 9-18 * * 1-5`), with times at least 15 minutes apart (ADR 0082). A Routine without `at` runs only on `run-routine`. `off` lists the Workspaces whose `.muxa/routines.toml` has Routines while `[workspace.routines] enabled` is not true: none of them run or appear in `routines`. `count` is the number of `[[routine]]` tables. A client turns them on with the `routines-gate` op of `settings-update`. `last`: `{"at","ok","reason"?,"published"?}`, the newer of the last success and failure. Times are Unix seconds.

### Inbox

ADR 0066. `Inbox`: `{"revision","items":[Item...],"done":[Item...],"bots"?:[{"workspace","bot","done","deleted","day"?,"added"}]}`. `added` is how many of the Bot's items counted against its `daily_items` on `day`, the host's local date (ADR 0086); its "N more" item and its talks' proposals do not count. Servers before ADR 0086 send neither. `items` are the open items in the user's order; `done` the last 100 done, newest first. Each change adds 1 to `revision`.

`Item`: `{"id","kind","title","note"?,"workspace"?,"due"?,"later_until"?,"created","source","terminal"?,"routine"?,"place"?,"links"?,"proposal"?,"bot"?,"settings_proposal"?,"suggestion"?}`. `settings_proposal` is on a Decide item of a talk with a Bot (ADR 0083); `suggestion` is the Capture assistant's (ADR 0084). ADR 0082: `links` are up to 10 `{"url","title"?}`, `http` or `https` pages the item is about; `proposal` is `{"action","target","content"}` on a Decide item, which Start Task takes as its goal, and muxa never performs by itself; `bot` names the Bot of `workspace` whose run made the item. `bots` counts, per Bot, its items that ended Done and those deleted while open. A Bot adds at most its `daily_items` a day; the rest go into one Read item titled `N more from <bot>`, whose note lists their titles.
- `kind`: `review`, `reply`, `decide`, `read`, `todo`, `failed`.
- `due`: `YYYY-MM-DD`. `created` and `later_until`: Unix seconds.
- An item with `later_until` in the future stays in `items` and the client hides it. At that time the server puts it back on top, clears `later_until` and sends `inbox`.
- `source`: `capture`, `task`, `routine` or `v1`.
- `terminal`: the Task agent terminal that made the item (Review, and Failed of an agent or a Task Loop). `routine`: `{"workspace","name"}`, the Routine that made it. Items made before these fields have neither.
- `place`: `{"workspace","path"}`, where an older Task worked. A client offers Resume Task for it: `start-task` with that workspace, the title without `Resume: `, and `places: [{"kind":"existing","path"}]`, then `inbox-done`.

`parse-capture` reads a line by ADR 0066's rules: a first word `reply`, `read` or `decide` sets the kind (else `todo`); `#name` sets the Workspace only when one has that name; `due` takes `today`, `tomorrow`, a weekday (the next one, today included), `M/D` (the next one) or `YYYY-MM-DD`. The client shows the fields, lets the user change them, and sends `inbox-add`. A title is 1 to 300 characters; a note at most 4 KiB.

Items the server makes:
- **Review** (`source: task`): on `task-review`. An agent can send it for its own Task (`muxa-cli v2 review` uses `HC_TERMINAL_ID`). While one is open, another `task-review` for the Task makes none.
- **Failed** (`source: task`): a Task's agent ended with a non-zero code, on a signal it did not get from a kill, or was lost. Once per terminal. A clean exit or a kill makes none.
- **Read** and **Decide** (`source: routine`): a Routine's briefing and proposal entries; **Failed** for a Routine's failure. Once each. Entries from before the server first ran are not copied.

### Migration from v1

ADR 0076. `migrate-v1` reads the open Task records of protocol v1 (status `open` or `review`) in the daemon's state dir, `<state dir>/t-<id>.json`. Done and dropped records and Routine runs are skipped. It never changes or deletes a record, a worktree or a branch.

`V1Task`: `{"id","workspace","title","path","exists","status"}`. `path` is the record's folder or worktree, resolved when it is there; `exists` says it is. `status`:
- `new`: a dry run would add it;
- `added`: one To do item `Resume: <title>` with `source: v1`, `place` and a note that it came from the old muxa;
- `in-inbox`: an earlier run added it. Each item has the key `v1:<id>`, so no run adds one twice, also after it is done or deleted;
- `missing`: its place is gone; `no-workspace`: its Workspace is gone; `outside`: its place is no longer in its Workspace. None of these adds an item.

`added` counts the items this run added; with `dry_run` it is 0.

### Talking to a Bot

ADR 0083. `start-task` with `bot` starts a talk: a folder Task in `workspace` whose agent starts from the Bot's `soul.md` and `memory.md`, with the Bot's settings and its Routines in `.muxa/TASK.md`. `places` must be empty or one folder. `bot` names a Bot of the Workspace, or a valid name with no Bot yet: then the talk may propose adding it. The agent gets `muxa.bot`, the Workspace's connections and MCP servers like any Task, and Claude Code may not edit the Workspace's `.muxa/` folder. The talk is a Task: Chat, the Terminal and Close work as for any Task.

Each time the agent reports `done`, the host reads `.muxa/bot.json` in the Task's folder, when it changed:
- `memory`: the Bot's new memory, at most 16 KiB. It replaces `memory.md`, and the earlier one is kept, as after a run.
- `proposal`: `{"summary","ops":[Op...]}`, 1 to 20 settings ops. Only `bot-add` and `bot-update` of this Bot, and `routine-add`, `routine-update` and `routine-remove` of this Bot's Routines in this Workspace, are allowed; a Routine it adds is added turned off. `members` of a `bot-update` may name only Bots of the Workspace. Anything else refuses the whole proposal, and the reason goes to `.muxa/CHECK.md` for the agent.

An accepted proposal becomes one Decide item: `title` is `<bot> proposes: <summary>`, `note` lists the ops one per line, and `settings_proposal` is `{"revision","ops"}`, with the settings' revision when the item was made. `bot`, `workspace` and `terminal` are set. `inbox-apply` sends the ops as `settings-update` with that revision and marks the item Done. When the settings changed since, it is `conflict` and nothing changes; the user asks the Bot again. Dismiss is `inbox-done` or `inbox-delete`.

### Capture assistant

ADR 0084. When `[assistant] enabled = true` in `workspaces.toml`, each item added by `inbox-add` gets one short run of the assistant: Claude Code in `<state dir>/assistant/runs/<item>/`, with no Workspace, no connections and no MCP servers. It reads the item, the Workspace names, each Workspace's Bots with their descriptions, its own `soul.md` and `memory.md`, and its last 30 decisions. Its suggestion becomes the item's `suggestion`:

`{"kind"?,"workspace"?,"due"?,"note"?,"split"?:[title...],"task_goal"?,"bot"?}`. `kind` is `todo`, `read`, `decide` or `reply`; `workspace` an existing Workspace; `due` `YYYY-MM-DD`; `split` 2 to 5 titles; `task_goal` a goal for Start Task; `bot` a Bot of the suggested Workspace, or of the item's. A run that suggests nothing leaves the item as it is.

`inbox-suggestion` with `accept: true` applies `kind`, `workspace`, `due` and `note` (after the item's own note); with `split`, the parts become new items in its place and the item becomes Done. `task_goal` and `bot` change nothing on the server: the client offers Start Task with the goal, and Ask `<bot>`, a talk (`start-task` with `bot`) whose goal is the item. Accept or dismiss clears `suggestion`, and the assistant keeps the decision to learn from.

### A Bot's work

ADR 0086. Clients show each Bot on a page of its own. Every client groups the data it already has by these rules, and the demo servers answer them the same way. For a Bot `b` of Workspace `w`:

- **Items:** open Inbox items with `bot` = `b` and `workspace` = `w`. Later items count once they are back.
- **Terminals:** live terminals with `muxa.workspace` = `w` and `muxa.bot` = `b` (its talks, runs, and its runs as a member), or `muxa.lead` = `b` (its members' runs).
- **Needs you:** its items, then its terminals whose `status.state` is `blocked` or `waiting`.
- **In progress:** its other terminals.
- **Planned:** its enabled Routines with a `next_due`, soonest first. A Routine with `on: "review"` shows as "On review".
- **Recent:** the items of `done` with `bot` = `b` and `workspace` = `w`. `done` keeps the last 100 of all items, so the list may be shorter than `BotCount.done`.
- **Status:** "needs you" when Needs you is not empty; else "running" when it has terminals; else "paused" when the Bot is turned off; else "idle".

Every client's own "needs you" list (the header pill, the next-waiting key) counts every live agent terminal (`muxa.role` = `agent`) whose state is `blocked` or `waiting`, of any `muxa.kind`. A Routine run or a team member waiting on a menu is the user's to answer in its terminal. `chat-open` serves Task agents only, so the approval card (ADR 0009) shows for Tasks.

A settings proposal of a talk is an item with `terminal` = the talk's agent. A client shows it beside that talk and applies it with `inbox-apply`, the same item as in the Inbox.

### Task actions

ADR 0072. The actions of a Review or Failed item, and the Workspaces a client offers when it starts a Task.

`Workspace`: `{"name","root","repos":[...],"agent"?,"loop","connections":[Use...],"mcp":[Use...],"branches"?:{repo: branch}}`. `branches` gives the branch each repo's own checkout is on, by its name in `repos`; a repo on a detached HEAD is missing.
- `repos`: the Workspace's repos as `start-task` takes them, relative to the root, or a listed repo's folder name.
- `agent`: the program name of the Workspace's agent, such as `claude` or `codex`. Never its arguments or environment.
- `loop`: `null` when the Workspace runs no Task Loops, else `{"checks":"any"}` or `{"checks":["make test",...]}`.
- `connections`: names only. A connection's variable, account and secret never cross the wire.
- `mcp`: the `[[workspace.mcp]]` servers, by name only. Their command, URL and variables never cross the wire.

`Use` says who uses a connection or an MCP server (ADR 0069 §7, ADR 0075): `{"name","default","used_by":{"routines":[...],"tasks":[...]},"last_used"}`.
- `default` is always `true` since ADR 0075: every agent of the Workspace gets it. It stays so older clients keep decoding.
- `used_by.routines`: every Routine in the Workspace's `routines.toml`.
- `used_by.tasks`: the ids of every live Task agent terminal of the Workspace.
- `last_used`: Unix seconds when the daemon last started an agent with it (a Task, a restart or a Routine run), or `null`. The host keeps the times in `<state dir>/usage.json`, names only.
- A server before ADR 0069 §7 sends only `name` and `default`; clients read the rest as empty and `null`.

`inbox-approve` closes the Task of an open Review item that has `terminal` (ADR 0068 §7, ADR 0043):
1. With `worktree: "remove"`, the host checks each branch worktree on the agent's `muxa.places`. Uncommitted changes, or a branch whose commits are not in the default base (`origin/HEAD`, else the main checkout's branch) or the main checkout's branch, is `would-lose-work`. Its message says which. Nothing changes.
2. A live Task Loop of the agent stops. The agent is killed and its terminal removed.
3. Each branch worktree goes with `git worktree remove`, its branch with `git branch -d`. With `force`: `git worktree remove --force` and `git branch -D`. Read-only places and the folder stay. Only places on the label are touched, so worktrees muxa did not make never are.
4. The item is Done.

A Review item with no `terminal`, or another kind, is `bad-request`. A terminal that is gone is `not-found`; the client offers Done instead.

`close-task` runs steps 1 to 3 for the agent `terminal`, live or ended, then marks every open item with that `terminal` Done. A terminal that is not a Task's agent is `bad-request`; one that is gone is `not-found`.

`inbox-request-changes` types `comment` into the live agent of an open Review item as the user's input, then Enter, and marks the item Done. A comment is 1 to 8192 bytes, of text, newlines and tabs; `\r\n` counts as a newline. One line is typed as it is; several lines go as one bracketed paste (`ESC [200~` … `ESC [201~`). An ended agent is `bad-request`. The comment never reaches the server's log.

`inbox-retry` marks an open Failed item Done after it runs again what made it:
- `routine`: the Routine runs now, as `run-routine`; `conflict` while it already runs.
- `terminal`, live agent: its Task Loop starts again with the same check and `reset` (a failed loop). No loop is `bad-request`.
- `terminal`, ended agent: the Workspace's agent starts again in the same working place, with the same places, labels and connections, but not in plan mode. The ended terminal is removed. A working place that is gone is `bad-request`.
- Neither, or another kind: `bad-request`.

### Settings

ADR 0074. A client reads and edits the host's `workspaces.toml`, each Workspace's `.muxa/routines.toml` and the Routines' instruction files. The files stay the source of truth, and a person can still edit them by hand. The daemon edits them in place, so comments and order stay, and reads them again on its next use.

`Settings`: `{"revision","read_only","problem","workspaces":[SettingsWorkspace...],"workers":[SettingsWorker...],"assistant"?}`.
- `assistant`: `{"enabled","worker"?,"soul_text","memory_text"}`, the Capture assistant (ADR 0084): `[assistant]` in `workspaces.toml`, and `assistant/soul.md` and `memory.md` beside it. Absent from an older server.
- `revision`: a hash of the files' bytes. It changes when any of them changes, by hand too.
- `read_only`: the daemon runs with `--settings-read-only`; `settings-update` is refused.
- `problem`: why `workspaces.toml` cannot be used as it is now, or `null`. The view still shows what the file says.
- Paths and schedules are shown as written in the files, `~/` included.

`SettingsWorkspace`: `{"name","root","repos":[...],"found_repos":[...],"hidden_repos":[...],"park_after","agent":{"command","env":[{"key","set"}...]},"loop","routines_enabled","direct_writes","connections":[SettingsConnection...],"mcp":[McpSpec...],"routines":[SettingsRoutine...],"bots"?:[SettingsBot...],"routines_problem"}`. `repos` are the listed ones; `found_repos` are the repos the scan finds under the root (ADR 0031), hidden ones too, as named under it; `hidden_repos` are found repos the Workspace leaves out of its repo list, so `workspaces`, Start Task and `all-read` skip them. A listed repo is never hidden.
- `park_after`: `"30m"`, `"12h"`, `"2d"`, or `null` (ADR 0034). `agent.command`: `null` means the daemon's `--task-command`.
- `agent.env` lists the keys of `[workspace.agent] env` with `"set": true`. A value never crosses the wire from the server.
- `loop`: `null` (no Task Loops), else `{"checks":"any"}` or `{"checks":["make test",...]}`.
- `routines_enabled` and `direct_writes`: `[workspace.routines]`.
- `routines`: the Routines of `<root>/.muxa/routines.toml`, also while `routines_enabled` is false. `routines_problem`: why that file cannot be used, or `null`.

`SettingsConnection`: `{"name","env","account","access","secret"}`. `account` is a label or `null`; `access` is `"read"`, `"write"` or `null`. `secret` is `"set"`, `"missing"`, `"locked"` (the host's Keychain is locked) or `"unavailable"` (the host has no macOS Keychain). The secret itself never crosses the wire from the server.

`ConnectionSpec`, in ops: `{"name","env","account"?,"access"?}`. Every agent of the Workspace gets every connection and MCP server (ADR 0075), so neither has `default`, and a Routine names none. A `default` sent by an older client is ignored.

`McpSpec`: `{"name","command"?,"args"?,"url"?,"type"?,"env":{},"headers":{},"connections":[...]}`, as `[[workspace.mcp]]`: either `command` with `args`, or `url` with `type` `"http"` (default) or `"sse"`. Nothing here is a secret.

`SettingsRoutine`: a `RoutineSpec` plus `"instructions_text"`, the instruction file's content, or `null` when it cannot be read.

`RoutineSpec`: `{"name","at"?,"schedule"?,"timezone"?,"worker"?,"instructions","project"?,"enabled","items"?,"direct_writes"?,"bot"?,"read_only"?,"on"?}`, as `[[routine]]` with `[routine.output]` flattened. `bot` names a `[[bot]]` of the same file; `read_only` denies Claude Code's edit tools to the run (ADR 0082). `on` is `review` or absent (ADR 0084); a Routine with `on` has no `at` or `schedule`. Editing a connection, an MCP server or a Routine removes the ignored `default`, `connections` and `mcp` keys from the file. `instructions` is the file's path relative to `.muxa/`.

`SettingsBot`: a `BotSpec` plus `"soul_text"` (`.muxa/bots/<name>/soul.md`, empty when there is none), `"memory_text"` (`memory.md`, which the Bot's runs write) `"memory_versions"`, how many earlier memories `bot-memory-restore` can bring back, and `"memory_history"`: `[{"version","at"}]`, newest first, `at` in Unix seconds when that memory was replaced (absent from servers before ADR 0086). A text is `null` when it cannot be read. The Soul counts in `revision`; the Memory does not, so a run does not make a client's edit stale.

`BotSpec`: `{"name","description"?,"worker"?,"daily_items"?,"enabled","members"?,"handoffs_per_day"?}`, as `[[bot]]` in `.muxa/routines.toml` (ADR 0082). `name` is 1 to 64 lowercase letters, digits, `-` or `_`. `daily_items` is 1 to 50, absent 3. A turned-off Bot turns off its Routines. `members` makes the Bot a team lead (ADR 0085): up to 6 other Bots of the same file, none of which leads a team. `handoffs_per_day` is 1 to 50, absent 10.

`SettingsWorker`, also `WorkerSpec` in ops: `{"name","description"?,"agent","args":[...],"instructions"?,"workspaces"?,"loop"?:{"check","max_attempts"?,"timeout"?}}`, as `[[worker]]`. `agent` is `claude-code` or `codex`.

`settings-update` applies `ops` in order. Each `Op` is `{"op", ...}`:

| `op` | Fields | Effect |
|---|---|---|
| `workspace-add` | `name`, `root`, `repos`? | A new `[[workspace]]` at the end |
| `workspace-update` | `workspace`, `root`?, `repos`?, `hidden_repos`?, `agent_command`?, `park_after`? | Sets the fields that are present; `null` removes `agent_command` or `park_after` |
| `workspace-remove` | `workspace` | Removes the Workspace and the Keychain secrets of its connections. Its folder and `.muxa/` stay |
| `env-set` | `workspace`, `key`, `value` | Sets one `[workspace.agent] env` variable |
| `env-unset` | `workspace`, `key` | Removes it |
| `loop-checks` | `workspace`, `checks` | `"any"`, a list of commands, or `null` for no Task Loops |
| `routines-gate` | `workspace`, `enabled`, `direct_writes`? | Sets `[workspace.routines]` |
| `connection-add`, `connection-update` | `workspace`, `connection`: ConnectionSpec, `secret`? | Adds a connection, or changes the one named `connection.name`. `secret` goes to the Keychain |
| `connection-secret` | `workspace`, `name`, `secret` | Replaces the connection's Keychain secret |
| `connection-remove` | `workspace`, `name` | Removes the connection and its Keychain secret |
| `mcp-add`, `mcp-update` | `workspace`, `mcp`: McpSpec | Adds a server, or replaces the one named `mcp.name` |
| `mcp-remove` | `workspace`, `name` | |
| `routine-add`, `routine-update` | `workspace`, `routine`: RoutineSpec, `instructions_text`? | Adds a Routine, or changes the one named `routine.name`. `instructions_text` replaces the instruction file's content |
| `routine-remove` | `workspace`, `name` | Removes the Routine. Its instruction file stays |
| `bot-add`, `bot-update` | `workspace`, `bot`: BotSpec, `soul_text`? | Adds a Bot, or changes the one named `bot.name`. `soul_text` replaces its `soul.md` |
| `bot-remove` | `workspace`, `name` | Removes the Bot. Its Routines must go first; its folder stays |
| `bot-memory-restore` | `workspace`, `name`, `version` | Brings back kept memory `version`, 1 the newest of 5; the current one is kept in its place |
| `assistant-update` | `enabled`, `worker`?, `soul_text`? | Sets `[assistant]`; `soul_text` replaces `assistant/soul.md` beside `workspaces.toml` |
| `worker-add`, `worker-update` | `worker`: WorkerSpec | Adds a worker, or replaces the one named `worker.name` |
| `worker-remove` | `name` | |

Names are not renamed: a Keychain account, a Routine's state and a Task's labels hold them. To rename, remove and add.

Rules:
1. `revision` must be the current one, else `conflict` and nothing changes. The client reads `settings` again and shows the user what changed.
2. The ops apply all or none. The result must pass the same checks as loading the files: a root and listed repos that exist, names, variables, loop checks, the connections an MCP server needs, workers and their workspaces, schedules and time zones. The Routines of every Workspace an op touched must load, also while `routines_enabled` is false. Any failure is `bad-request`, its message starts with the op's index (`ops[2] connection-add: …`), and nothing changes.
3. `value` and `secret` are write-only. They are never sent back, never logged, and never in an error message. A `secret` goes only to the host's macOS Keychain (service `muxa`, account `<workspace>/<name>`), at most 1800 bytes of one line. An env `value` goes only into `workspaces.toml`, as before.
4. An `instructions_text` or `soul_text` is at most 12 KiB of UTF-8 without NUL. The file must stay inside `.muxa/`, also through a symlink.
5. Each file is written whole to a temporary file and renamed over the old one; the old one is kept as `<file>.bak`. A file keeps its mode; a new file gets 0600.
6. With `--settings-read-only`, `settings-update` is `read-only` and nothing changes.
7. A Keychain or file failure is `backend`. The Keychain is written before the files, so a failed file write can leave a new secret without its connection.

`[codex]` is not in the view and no op changes it; edit it by hand.

### Error codes

`bad-request`, `version`, `not-found`, `backend`, `closed`, `conflict`, `would-lose-work` (closing a Task would lose work; nothing changed), `read-only` (the host locked its settings with `--settings-read-only`).

## Not yet specified

Review from a draft PR, adding a place to an open Task (ADR 0068), and everything from protocol v1 beyond terminals: Tasks, Inbox, Reader, Chat, Routines. Each is added here when it is built.
