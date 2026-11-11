# 0061: Retry an assistant run's failure

## Status

Accepted on 2026-11-12. It amends ADR 0048's addendum of 2026-11-11 (an assistant run's `failure` item offers ✓, Ask Agent and Later) and ADR 0056 §2 (the secretary's give-up waits until the agent runs again). No new wire value: hosts stay at `work:18`.

## Context

- Three things make a `failure` item without a Task: a failed request step and a failed plan (ADR 0048, addendum), and the secretary's restart cap (ADR 0056 §2).
- Each item says why it failed. Nothing on it runs the work again. The user has to ask the Work item again, press Re-rank, or restart the daemon.
- A Task's `failure` already offers `retry`, which restarts its agent. The apps, the CLI and muxa for tmux label it Retry and put it first among the decisions.

## Decision

### 1. `retry` on an assistant run's failure

An assistant run's `failure` item lists `retry, dismiss, ask-agent, later`. `retry` keeps its meaning: run the failed thing again. The item's stored key names what failed:

| Key | Retry |
|---|---|
| `failure:<request>:<version>:<step>` | asks the request again as a new version: the same workspace, scope and purpose, and the member its first step named. It goes through `request-assistant`'s own code, so the budgets and the delegate rules apply. |
| `failure:<plan>` | runs a new plan now, as `plan-now` with the failed plan's minutes. The overall budgets apply. |
| `failure:secretary:<time>` | writes `retryAt` to the secretary's state. The daemon's next check resets the restart cap and starts the chat. Only the daemon starts the secretary, so a retry never opens a second session. |

The reply is the one the request would send (`work-saved` or `plan-saved`), then `inbox-saved` with the item answered `retry`, then `result`. A refusal, for example a budget, is the `result`'s error, and the item stays open.

### 2. Once

The retry is kept as the item's approval, keyed by the item's key, as an approved proposal is (ADR 0049 §4). A second `retry` returns what the first made and starts nothing. A retry that is still running answers `conflict`. When the request was asked again another way, or a newer plan replaced the failed one, the item is `resolved` with that reason and nothing starts.

### 3. No new action

A new action such as `retry-run` would need every client to learn it. `retry` already means "run it again" on a `failure`, and every client shows it. `approve` would also work, but on a `failure` it would read as approving the failure.

## Consequences

- No wire change and no level bump. A host before this ADR does not list `retry` on these items, so no client sends it there.
- The apps need no change to offer Retry. They show the item's `answers` and send `update-inbox retry` already. They should accept a `work-saved` or `plan-saved` before the `inbox-saved` and refresh the Work item or Today from it, as for `approve`.
- A retried request whose run fails again makes a new item: its key has the new version.
