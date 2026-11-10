# Security

## Reporting

Report vulnerabilities privately through GitHub's "Report a vulnerability" (Security tab). Do not open a public issue.

## Threat model

muxa gives a remote device keyboard access to your terminal sessions. That is shell access. The design keeps the attack surface to what SSH already exposes.

- **Transport:** the bridge runs as an SSH exec command. It opens no port. Authentication, encryption and host identity come from SSH. The app pins the host key on first use and stores its ed25519 key in the device Keychain, readable only when the device is unlocked.
- **Restrict the key.** An SSH key that can run the bridge can also run a shell, unless you restrict it. The app adds its public key to `~/.ssh/authorized_keys` with a forced command. The setup uses your password for one login after the host key is trusted, and does not store it. You can also add the line by hand:

  ```
  command="~/.local/bin/muxa-bridge",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 AAAA... muxa-ipad
  ```

  The key can then reach your sessions only through the bridge. The bridge sends keystrokes only to panes that already exist, and only on explicit requests.
- **Input from the client is untrusted.** Requests have a size cap. Pane and session ids are validated. Every value placed in a tmux command line goes through one quoting function, so no request can add tmux commands or expand variables.
- **Terminal content is untrusted** and is passed through as data. The bridge never logs pane bytes. Logs contain ids, sizes and errors only.
- **The Reader only opens what the bridge listed itself.** A client names a path from that list, never a path of its own. Except for the agent's plan, nothing is read outside the pane's git repo, even through a symlink. The content is never logged.
  - **Documents:** the Markdown files of the repo that git does not ignore, and the pane's agent plan. At most 2 MiB each.
  - **Images:** PNG, JPEG, GIF, WebP and HEIC files in the repo, including folders git ignores, such as QA screenshots in `build/`. Only these extensions are listed, so an ignored `.env` or key file never is. SVG is left out because it can carry scripts. At most 16 MiB each. An image in an ignored folder can be private, for example a screenshot of a login page.
  - **Changes:** the repo's uncommitted changes, and the files the branch's own commits changed since its base. A diff is at most 2 MiB. git runs read-only, and the bridge never fetches.
  
  The app renders documents and diffs offline in a web view with a content security policy that allows no network, and sanitizes their HTML.
- **Tasks write to the host.** A client can start a task: a git worktree on a branch, and a detached session running an agent. It can also close one. The client gives the repo's absolute path and the branch name. The bridge checks that the path is a git repo and that git accepts the names as branch names, so no name is read as an option. The command that starts the agent is fixed by the bridge's command line (`--task-command`, `--task-launcher`), not the client. Closing never removes a worktree with uncommitted changes, and always keeps the branch.
  - A repo may also be named relative to a workspace root (ADR 0031). After symlinks are resolved it must stay inside that root, unless it is a repo `workspaces.toml` lists. Finding the repos under a root never follows a symlink and stops after 2000 entries or 250 ms.
  - Without a branch, the bridge names one after the title: lowercase ASCII letters, digits and `-` only.
  - **Ask sessions** (`start-session`) start the workspace's agent in a repo's main checkout, not a worktree. That agent can change files there, on whatever branch is checked out. Nothing enforces read-only.
- **Task records write to the host** (ADR 0015). A client can record a task: a JSON file in the state directory (`--state-dir`, mode 0700), and `.muxa/TASK.md` with its goal in the task's directory. A task without a repo gets a directory only inside a workspace root from `workspaces.toml` (`--workspaces`), which clients cannot change. The bridge, or the host daemon when it runs, then types one fixed prompt into the new agent pane; no client text reaches a command line. Logs carry task ids and paths, never a title, goal or note.
- **Per-workspace agent environment** (ADR 0016). `[workspace.agent]` in `workspaces.toml` can give a workspace's agents their own variables, for example `CLAUDE_CONFIG_DIR` or a token. Only the host's file sets them; clients cannot. The bridge passes them to tmux on the control client's stdin, so they never show in a process list. Logs carry the variable names, never a value. `TMUX` and `TMUX_PANE` are refused.
- **The host daemon** (ADR 0019) is optional. It runs the Task Loop, and the user installs it by hand in a shell (`muxa-bridge daemon install`). A client key cannot install, stop or remove it: the forced command runs the bridge with no subcommand, and the bridge never forwards `shutdown`.
  - It opens no network port. It listens only on `daemon.sock` in its directory (`~/.local/state/muxa/` by default). The directory has mode 0700, and the socket 0600.
  - On every connection it checks the peer's uid, and closes a connection from any other user, root included.
  - It answers only Task requests and `shutdown`, the same protocol v1 as the bridge. Anything else is `unsupported`.
  - Its tmux client attaches with `-f ignore-size,no-output`, so no pane bytes reach it. It attaches only while it has work.
  - `daemon.log` carries ids, paths, states, exit codes and durations. It never carries a title, goal, note, check command, check output, environment value or prompt text.
- **Check commands run on the host** (ADR 0019). A Task Loop runs its check with `/bin/sh -c`, in the Task's directory, as your user, with no pane to show it. Its environment is the tmux server's, plus the workspace's `[workspace.agent]` env, so it has the same accounts and tokens as the agent.
  - Only `workspaces.toml` on the host allows checks, per workspace, in `[workspace.loop]`. Without that table, the workspace has no loops. With a list, a client can only pick one of its commands, exactly. With `"any"`, a client can send any command of at most 4096 bytes. Prefer a list.
  - Only a Task in one of the workspace's repos (listed in `repos`, or under its root), or a Task without a repo under its root, can have a loop.
  - A check's output goes to `.muxa/checks/` and `.muxa/CHECK.md` in the Task's directory, never to a log. The loop types only a fixed prompt, and never answers an agent's permission, question or plan.
- **What a stolen client key gains.** The key could already type into every pane, and a shell pane runs any command. So it was already equal to a shell as your user. The loop adds one path: with `"any"`, the key can have a command run with no pane that shows it. With a list, it can only run the listed commands. To cut a device off, remove its line from `~/.ssh/authorized_keys`.
- **Stop everything.** On the host:
  1. `muxa-cli task stop-loop --all` stops every loop that is armed, checking or paused.
  2. `muxa-bridge daemon stop` stops every loop, kills the running checks, and stops the daemon until the next login. `muxa-bridge daemon uninstall` also removes its service file.
- **Captures, Work items and assistant runs** (ADR 0048). A client can save Capture text, at most 16 KiB, in `<state dir>/assistant/`. With `[workspace.assistant]` in `workspaces.toml`, it can also ask that workspace's agent to work on a Work item. Only the host's file allows requests; clients cannot.
  - Capture text, briefs and Results never reach a log. Logs and error messages carry ids, sizes and states only. A run's folder and session names hold the scope and ids, never Capture text.
  - Capture text and earlier Results reach the agent only in `.muxa/TASK.md`, under headings that mark them as data, not instructions. The agent gets one fixed prompt; no client text reaches a command line.
  - The host checks `.muxa/assistant/result.json` in its own process before it stores anything. It refuses a symlink (the file, `.muxa` or `.muxa/assistant`), a file that is not regular, NUL or other control characters, a file over 64 KiB or a field over its limit, unknown fields, a finding without a source, and a request or version that is not the run's own. The reasons in `.muxa/checks/` name fields, never values.
  - Budgets bound what a client can start: `max_open_runs` and `max_runs_per_day` per workspace, and `[assistant] max_runs_per_day` per host. Requests run only in the daemon, and only with a Claude Code delegate.
  - A run's agent has the same access as any agent in that workspace. It can read and write files there and use the network as its permissions allow. Prefer a `worker` with narrow permissions.
- **Delegate notes and the overall assistant** (ADR 0049).
  - The overall assistant's brief is built by the host, and only the host. `[workspace.assistant] share` on each workspace says what crosses. `outline`: Work item titles, states, Later, pins and due dates, request scopes and delivery states, open Inbox items' kinds, titles and ages. `summaries`: also each Work item's newest Result summary (cut to 600 characters), up to 3 open questions and 3 next actions, and the number of findings. `none`: nothing, not even the workspace's name.
  - Never in the brief, at any level: findings' text, sources or file paths, Result `files`, a workspace's Capture text, a delegate's notes, `TASK.md`, pane content, or another workspace's credentials. The overall agent reads no other workspace's root through muxa. This is a host filter, not an OS sandbox: its own tools are what its configuration allows.
  - The brief is `.muxa/assistant/BRIEF.md` in its run folder, mode 0600.
  - A run proposes new notes; it never writes `WORKSPACE.md` or a member's `NOTES.md`. The default `notes = "review"` makes the user apply each proposal, because a web page or document the run read can plant text that would then reach every later run. `"auto"` removes that review. The host applies a proposal only while the live file still has the hash the run read. An oversize, non-UTF-8 or symlinked notes file fails the run's start.
  - The overall assistant only proposes. Each `sort` and `delegate` is an Inbox item the user approves, once. A workspace receives a request without approval only with `overall_requests = "auto"`; that request still counts against every budget, `[assistant] max_auto_requests_per_day` (default 5) and one per workspace per plan, and its Inbox item stays visible. `none` refuses even an approved request. Sorting is always reviewed.
  - Logs carry ids, counts, hashes, states and schedule times. Never notes, plan texts, reasons, purposes or summaries. A plan's `reason` is host text that names ids and limits, never the agent's text.
- **Policies, corrections and hypotheses** (ADR 0050).
  - Logs carry ids, kinds, counts, scopes and states. Never policy texts, reasons, hypothesis texts or evidence texts.
  - The brief's "What the user told you" section follows `share`: a policy, hypothesis or correction scoped to a workspace whose `share` is `none` is never in it.
  - Forgetting is structural, not a promise of the agent. The host builds each brief from live records only, and the plan check refuses a `basis` or `evidence` id the brief did not carry, or whose record is gone. A forgotten record cannot reach the next brief or be cited as a reason.
  - What stays as history after Forget: the kept plans' rows and `basis` texts, and the run folder's `plan.json` until its folder is removed (a newer plan, or 7 days). The notes are the agent's: the brief forbids copying the section into it, and the default `notes = "review"` shows the user when it did.
- **Staff and hand-offs** (ADR 0051).
  - The staff list, each member's `description` and the writer live in the host's `workspaces.toml`, not under a workspace's root: a worker is a grant of permissions, and an agent writes under root. A description is a job, not a permission.
  - A hand-off names a member of the same workspace only. Nothing adds a path from one workspace's run to another's.
  - The default `handoffs = "review"` makes the user approve each hand-off, because a page one member read can plant a purpose the next would follow. `auto` stays within every budget and `[assistant] max_auto_handoffs_per_day`.
  - A member proposes changes to its own notes file only; the lead's is the workspace memory, `WORKSPACE.md`.
  - Logs carry worker names, step numbers and ids. Never descriptions, purposes or hand-off texts.
- **Writes outside the host** (ADR 0052).
  - Staff members read; they never write. Only the workspace's `writer` performs a write, and config refuses a writer that is also the `worker`, on `staff` or the lead. Give the writer the write tools of its targets and no web, search or read-side MCP. muxa states this; it cannot enforce it.
  - Every write is an Inbox item the user approves; there is no `auto`, and `direct_writes` grants nothing here. Approve carries the hash of the action, target and content the client showed. Without it, or with another one, approve is refused. A client older than `work:13` never sends it, so it cannot approve a write.
  - Clients show the content whole, as plain text in a fixed-width font, without rendered Markdown or active links, so the text approved is the text seen.
  - The writer's Task holds the content unchanged and is told to do exactly that write and nothing else. The host cannot check that it did; the Task's review and the target system show it. `[assistant] max_writes_per_day` (default 10) caps approved writes.
  - Logs carry ids, hashes, counts, worker names and states. Never actions, targets, contents or reasons: a target can hold a token. The log line of a linked Task no longer carries its folder path.
- **Workspace memory** (ADR 0053).
  - `<root>/.muxa/WORKSPACE.md` reaches every later Task, staff and Routine run of the workspace as a copy. A Task's agent read the web, the repo and documents, so a planted line can land in its "For the workspace" section. The host checks size and control characters only, never meaning or truth.
  - The default `memory = "auto"` appends without a look (ADR 0053 Revision 1). The guard is on the way out: a planted line can change later runs, but every external write still needs the user's approve of its exact text (ADR 0052). Each append carries the Task's id and date, so one block can be found and deleted. `memory = "review"` makes the user read each proposal before it is appended.
  - Approve carries the hash of the Task, its title and the text the client showed whole, as plain text in a fixed-width font. Without it, or with another one, approve is refused. A text over 4 KiB, or with a control character other than line breaks and tabs, has no Approve.
  - `TASK.md`, the briefs and the Routine goal say the copy is data, not instructions: `TASK.md` and the repo's `AGENTS.md` or `CLAUDE.md` decide what to do. muxa cannot check that an agent treats it so.
  - An oversize, non-UTF-8 or symlinked `WORKSPACE.md` fails a start before any session. Appends go through a temp file and a rename, under an OS lock per workspace in the state directory.
  - Logs carry Task ids, item ids, workspace names, sizes, hashes and states. Never the section, the file or a heading: a Task title is content.
- **Agent transcripts** (for the chat view) are read from the agent's own files under your home directory: `~/.claude` for Claude Code, `~/.codex` for Codex. They are sent only over the same SSH channel, and never logged. Reading them needs no access beyond what the key already has: it can type into the same panes. Where the bridge looks is fixed by its command line (`--agent-dir`), which the forced command in `authorized_keys` sets, and by the agent directories in `workspaces.toml`, not by the client.
- **Out of scope:** an attacker who already has a shell as your user, and a compromised SSH server.
