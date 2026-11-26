# 0103: Host resources in Settings and the status bar

Status: proposed (2026-11-26).

## Context

- The user: "can muxa also shows the host resource? what is costing CPU / Memory?" The apps show nothing about the host.
- A check on the Mac mini host (24G) on 2026-11-26 found 23G used, 7.6G compressed and high swap counts. The cost was 31 Claude Code processes (about 7G, many in Tasks idle for 1 to 3 days), a booted iOS simulator (about 240 processes), the Docker VM (1.7G) and Chrome (2.4G).
- herdr-core already reports `pid` and `status_ms` for each terminal (`hc schema`, 0.1.21). muxa drops both.
- ADR 0102 added `disk`: the disk use of Task places and leftovers. No app shows it yet.

## Decision

1. **The host reports, the apps show.** `host` returns a `HostReport`. `host-watch` sends it again every 10 s as a `host` notification, until `host-unwatch` or the end of the connection. An app watches only while the summary or the Host page is on screen.
2. **The daemon samples only while asked.** One sample serves every session for 5 s. Sources need no new crate and no `unsafe`:
   - macOS: `ps`, `sysctl` (memory size, swap, load, `kern.memorystatus_vm_pressure_level`) and `vm_stat`;
   - Linux: `/proc/meminfo`, `/proc/loadavg`, `/proc/pressure/memory` and `ps`;
   - both: `statvfs` for disk free space.
3. **The report:**
   - CPU (% of all cores), load, memory (used, compressed, pressure), swap;
   - free space on the system disk and on the build disk (`[build] dir`, ADR 0102);
   - usage groups;
   - `alerts`: which values are past a limit.
4. **Usage is grouped by what the user can act on.** Each group has CPU %, memory (RSS) and a process count:
   - `task`: one per Task or talk. It covers the process trees of the agent terminal and its shells (same `muxa.dir`), from herdr-core's `pid`. It carries the agent id and `idle_since_ms`: herdr-core's `status_ms` while the agent reports `done` or `idle`.
   - `routine` and `member`: Routine runs and team members.
   - `claude-outside`: Claude Code processes that no muxa terminal holds.
   - `simulator`: CoreSimulator processes.
   - `docker`: Docker.
   - `vm`: virtual machines, such as the one Docker runs. It is its own process, so muxa cannot tell whose it is.
   - `app`: other apps, by the name of their `.app` bundle.
   - `other`: the rest.
5. **Only names cross the wire.** A group shows a bundle name or a process base name, never argv or the environment, since argv can hold secrets. Process data never reaches a log (invariant 4).
6. **The host sets the limits**, so all three apps agree. A value is in `alerts` when:
   - memory pressure is `warn` or `critical`;
   - CPU is at 85% or more for two samples in a row;
   - a listed disk has less than 10% or less than 20G free.
7. **Settings › Host** in every app shows:
   1. the figures;
   2. the Tasks and talks by memory, with how long each agent has been idle;
   3. the other groups;
   4. Disk: a Scan button sends `disk` (ADR 0102, the one source for disk use) and shows each Task's place and build size, then the leftovers.
   - A Task row opens the Task. Pause (`pause-task`) is added when it is on main.
   - Cleaning stays in `muxa-cli disk clean`.
   - The page names no ADR, request or make target. Those are for the code, not the user.
8. **A summary in the status bar:** `CPU 12% · Mem 96% · 41G free`. A value in `alerts` takes the warning color. A tap or click opens Settings › Host.
   - Mac: the foot of the Focus sidebar, under Bots, and the strip at the foot of Home.
   - iPad and Android tablet: the sidebar footer, under the host menu.
   - Phones: a chip beside the host menu in the Inbox title.

## Consequences

- The user sees which Tasks cost memory and how long they have waited, and can open or pause them.
- `TerminalInfo` gains `pid` and the time of the last status change. Every backend must fill them or leave them empty.
- A watching app costs the host one `ps` and a few `sysctl` calls every 10 s.
- Disk sizes need a scan (`du`), so they show only on request.

## Alternatives not taken

- **Resources inside the `watch` stream:** every app would pay for sampling, even with no summary on screen.
- **The `sysinfo` crate:** it adds a dependency and still gives neither macOS memory pressure nor compressed memory.
- **Limits in each app:** three copies that drift.
- **Disk sizes in each report:** `du` over every worktree is too slow for every 10 s.
- **Full command lines in the groups:** they would show more, but argv can carry tokens.
