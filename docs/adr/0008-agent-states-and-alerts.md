# 0008: Following agent states, and alerts inside the app

## Status

Accepted.

## Context

An agent elsewhere that waits for input, finishes or fails should not go unnoticed while the user works in another session on the iPad. Agent states live in pane options (tmux-agents' `@agent_state`) or in the agent's own files (Claude Code's session file). tmux reports no option changes, so the session list showed old states until it was listed again. iOS suspends the app in the background, and the app then closes its SSH connection. Reaching the iPad then needs a push service.

## Decision

- **The bridge looks every 2 seconds** at each pane's role and agent state, and sends `sessions-changed` when one changed. The time of a state is left out, so a refreshed time alone sends nothing. Clients list again, as they already do for structural changes. The protocol gains no message.
- **Panes without a state tag get the state their agent records**, through `AgentAdapter::find` (Claude Code: `status` in its session file). Processes are listed only when some pane needs it.
- **The app compares each new list with the last** (`alerts(since:shown:)` in `MuxaSession`) and shows a short banner for an agent that now waits, finished, or failed. A running agent that becomes idle counts as finished. The shown session and new panes are left out. Tapping the banner opens the session.
- **While the app is closed, muxa sends nothing.** The README shows how to point tmux-agents' `@agents_notify_cmd` at a push service such as ntfy.

## Consequences

- One `list-panes` every 2 seconds per connected client; `ps` and a few small file reads only with untagged agent panes.
- No server and no push certificate. The cost: no alert while the app is closed, unless the user sets up a notifier on the host.
- Rejected: a new message for state changes. Listing again is enough, and old clients keep working.
- Rejected for now: muxa's own push through APNs. It needs a relay server and Apple credentials, and the project has neither. It stays a candidate in `docs/plan.md`.
