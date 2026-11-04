# 0036: Routines live in the workspace, permissions stay on the host

## Status

Proposed. Changes ADR 0026 section 1 (where `[[routine]]` is written) and its trigger (a daily time becomes a schedule). The rest of ADR 0026 stands.

## Context

- Today every `[[routine]]` is in `~/.config/muxa/workspaces.toml`, with its workspace named in each table. The developer wants a workspace's definitions next to its work, as orch keeps them (`projects/<context>/tasks.yaml`, `mcp.json`, `CLAUDE.md`).
- `workspaces.toml` decides what an agent can reach (ADR 0015): the agent command and env (ADR 0016), loop checks run by `/bin/sh -c` (ADR 0019), and Routine `enabled` and `direct_writes` (ADR 0026). An agent of the workspace can write files under its root. So a file under the root must not grant any of these.
- orch's Lisk schedules use cron: weekdays (`0 8 * * 0,2-6`), a day of the month (`15 5 1 * *`), months (`45 5 1 1,4,7,10 *`). Each runs at most once a day. A Routine today has only a daily `HH:MM`.
- orch also has `guard:` commands (biweekly, "a meeting today"). A guard is a command the scheduler runs with no pane.

## Decision

### 1. Two layers

| File | Holds | Written by |
|---|---|---|
| `~/.config/muxa/workspaces.toml` | workspaces, `[workspace.agent]`, `[workspace.loop]`, `[[worker]]`, and the new `[workspace.routines]`: what is allowed | the host's owner |
| `<root>/.muxa/routines.toml` | `[[routine]]` of that workspace: what runs and when | anyone who can write the root, agents included |

```toml
# ~/.config/muxa/workspaces.toml
[[workspace]]
name = "Lisk"
root = "~/Projects/lisk"

[workspace.routines]
enabled = true          # this workspace's Routines may run
direct_writes = false   # a Routine may set direct_writes only when this is true
```

```toml
# ~/Projects/lisk/.muxa/routines.toml
[[routine]]
name = "situation"
schedule = "0 8 * * 0,2-6"
timezone = "Asia/Tokyo"
instructions = "routines/situation.md"   # relative to .muxa/
project = "Lisk"
enabled = true

[routine.output]
items = 20
```

- Without `[workspace.routines]`, or with `enabled = false`, the bridge does not read the workspace file and the workspace has no Routines.
- A Routine runs only when both `[workspace.routines] enabled` and its own `enabled` are true. The host's flag is the authority; the Routine's flag is a switch.
- `direct_writes = true` in the workspace file without the host's `direct_writes = true` is a configuration error for that Routine. muxa never quietly lowers it: the author must see that the write path is off.
- A `worker` must be a host `[[worker]]` allowed in this workspace (ADR 0022), as before.
- `instructions` is a path relative to `<root>/.muxa/`. After canonicalization it must stay inside `.muxa/`. No `~/`, no absolute path, no symlink out.
- `[[routine]]` in `workspaces.toml` is refused, with a message that names the new file. muxa is pre-alpha; one place for a Routine is simpler than a transition.

### 2. A broken workspace file breaks only that workspace

- The workspace file is read with the host file, on every load. Unknown keys are refused, as everywhere.
- An unreadable or invalid workspace file does not fail the load. That workspace lists no Routines and reports the error with the file path and the Routine name. The file holds no secrets (env stays in `workspaces.toml`), so a TOML parse error may quote it. Other workspaces, Tasks and loops work as before.
- Reason: an agent can write this file. A typo it makes must not stop the host for every workspace.
- A Routine that disappears or becomes invalid keeps its stored state and entries. When it comes back with the same name, its history continues (ADR 0026: the name is the id).

### 3. A schedule is cron for days, one time a day

`schedule` is five cron fields, `minute hour day-of-month month day-of-week`, with `timezone` (an IANA name).

- `minute` and `hour` are single numbers. A Routine runs at most once a day, so the occurrence-per-date model of ADR 0026 stays: one catch-up, DST, occurrence ids.
- The day fields take `*`, a number, a range `a-b` and a comma list. Day-of-week is `0`–`7`, both `0` and `7` are Sunday. No names, no steps.
- Day-of-month and day-of-week cannot both be restricted. Cron joins them with OR, which surprises readers. Use two Routines instead.
- A schedule that matches no day within four years is refused.
- `at = "HH:MM"` stays as the short form of `schedule = "MM HH * * *"`. `at` and `schedule` together are refused.
- The occurrence id keeps its form, `2026-11-03T09:00`.

### 4. No guard command

A condition such as "every second week" or "only when there is a meeting today" belongs in the instructions. The agent checks it and publishes nothing. An empty publish is a success.

## Consequences

- A workspace's Routines and their instructions can live in a git repo at `<root>/.muxa/`. The bridge's repo scan skips dot-folders, so it does not list `.muxa` as a repo.
- An agent of the workspace can change its Routines' schedule, instructions and item limit. It cannot enable Routines for a workspace, turn on direct writes, choose a disallowed worker, or point instructions outside `.muxa/`. Changed instructions are prompt input for the next run. A user who wants them reviewed keeps `.muxa/` in git, or denies edits there in the agent's own permission settings.
- Running an agent to decide "not today" costs one run. That is accepted for the few conditional Routines, against a daemon path that runs arbitrary commands.
- Rejected: everything in `workspaces.toml`. It keeps a workspace's work far from the workspace.
- Rejected: everything under the root. An agent could grant itself loop commands, env and direct writes.
- Rejected: full cron. Several runs a day need a new occurrence model. No migrated Routine needs it.
- Rejected: an include path from `workspaces.toml` to any file. A fixed path per root is one less setting and cannot point into another workspace.
