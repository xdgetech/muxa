# The tmux Inbox: where the time goes

Measured with `scripts/bench/inbox-bench.sh` on 2026-11-17, Apple silicon, tmux 3.7c, release builds.
- Input: a copy of a real state dir (428 task records, 255 item files) and a 363 KB Inbox JSON (45 open items with long Japanese summaries).
- The bridge ran without a daemon, so every request ran in the bridge itself.
- Sessions were plain shells with no agents. Agent panes add `ps` and `~/.claude/sessions` reads to `List`, so real fetches cost more.

## One fetch: `muxa-cli inbox --json`

| Part | 5 sessions | 40 sessions |
|---|---|---|
| Whole fetch | 134 ms | 239 ms |
| Bridge start, `tmux -C` attach, hello (`routines --json`) | 26 ms | 20 ms |
| Sessions `List` (`muxa-cli list`) | 102 ms | 224 ms |
| `ListInbox` over the whole history (difference) | about 25 ms | about 25 ms |

- `muxa-cli inbox` sends `List` for the Routine runs' sessions. No muxa for tmux script reads `.runs`.
- `List` grows with the number of sessions. It runs `git rev-parse` per untagged session.
- Reading the store from disk is not the cost: about 25 ms for the whole history.

## The popup (`prefix + T`)

| Script | Time | When |
|---|---|---|
| `inbox.sh view` | 49–56 ms | every cursor move |
| `inbox.sh list` | 56 ms | every reload |
| `inbox.sh place` | 56 ms | every reload |
| `inbox.sh key` | 74–84 ms | every key |

- The JSON size does not matter. `list` takes 54 ms on 115 KB and 54 ms on 363 KB.
- The time is process start and jq compiling the program:
  - bash 3.2 parses the 65 KB `inbox.sh` in 18 ms;
  - `jq -n empty` takes 2.5 ms, or 17 ms through a mise shim.
- One key with an action costs about 600 ms or more: `key` + the answer (a bridge) + `relist` (a fetch, then `list`) + `place`.

## Idle: one sidebar per window

| | 5 sidebars, 30 s | 40 sidebars, 30 s |
|---|---|---|
| Script starts | 140 | 973 (about 32 per second) |
| Inbox fetches | 6 | 5 |
| tmux server CPU | 80 ms | 1130 ms |

- Each sidebar runs its own loop. Every 5 s it starts `inbox.sh cache` and `muxa.sh available` (about 40 ms) just to learn that another sidebar fetched.
- Every 10 s each sidebar's `refresh.sh` runs `tree.sh` (110 ms), which runs `inbox.sh rows` (jq over the whole JSON).
- Estimate: about 1.2 s of CPU per sidebar per minute. With 40 windows that is about 46 s per minute, most of one core. The cost grows with the number of windows, not with the number of items.

## After the fix (same bench, 40 sessions)

| | Before | After |
|---|---|---|
| One fetch | 239 ms | 81 ms: `List` runs only for a running Routine run |
| Preview on a cursor move | 50 ms (bash + jq) | 5 ms (`cat` of a pre-rendered file) |
| Previews per reload | none | 76 ms, one jq run for every row |
| Idle script starts, 40 sidebars, 30 s | 973 | 392: one Inbox poller per tmux server |
| Idle jq calls, 40 sidebars, 30 s | 138 | 18: the sidebar section is rendered once per list |

- A key in the popup still runs `act`, the answer, and a fetch with its render, about 300 ms.
- Idle work is now mostly each sidebar's own 10 s tree refresh (`tree.sh`, about 40 ms), which grows with the windows. It is outside the Inbox.
