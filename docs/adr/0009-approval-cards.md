# 0009: Approval cards: the adapter names the agent's menu

## Status

Accepted.

## Context

While an agent waits, Chat showed a fixed banner with 1 / 2 / 3 / Esc. Those keys were a guess at Claude Code's menu, and the user could not see what they approved. A spike with Claude Code 2.1.310 showed:

- While Claude Code waits, its session file says `waiting`, and the last transcript line is the tool call it waits on, with its full input. This holds for a permission, AskUserQuestion and ExitPlanMode.
- The menus are not in the transcript, and they change with settings and context. The permission menu had two to four options, for example "Yes, and switch to auto mode". The plan menu's first option was "Yes, and use auto mode" in one setup.
- Some keys picked the same choice in every menu seen: `1` picks the first "Yes", `Esc` denies a permission (and interrupts the turn) or keeps planning, and a digit picks a single question's option.

## Decision

- **The adapter owns the menu.** `ChatSource::prompt` returns what the agent waits on, as a `prompt` on `chat-entries`: the tool entry, a kind (`permission`, `question`, `plan`), and options with the tmux keys that pick each one. The client knows no agent's keystrokes.
- **Only keys that held in every menu seen are offered.** A permission gets Allow (`1`) and Deny (`Esc`). A question with one single-answer question gets its options by number. A plan gets Keep planning (`Esc`). Everything else in the menu stays in the terminal, one tap away on the card.
- **A prompt is sent only when the adapter is sure.** The session file says `waiting`, exactly one call of the turn has no result, and its menu is known. Otherwise there is no prompt, and the app shows the old banner (invariant 6).
- **Every message carries the current prompt, as with `status`.** A message without entries carries a prompt change. The protocol stays v1, behind the capability `chat-prompt`. The Swift client decodes an unknown kind as `other`, so a newer bridge does not break an older app.

## Consequences

- Plan approval is not one tap. Which mode `1` picks depends on the user's settings, so approving a plan opens the terminal.
- Several questions, several answers, and several open calls fall back to the banner until their keys are checked.
- A Claude Code update that renumbers a menu would make a button pick the wrong choice. The samples in `crates/muxa-bridge/tests/data/claude-code/` pin the version, and each new version should be checked against them.
- Codex records no approvals, so Codex panes get no cards.
- Rejected: reading the menu from the pane's screen. It is fragile, and roles and state never come from screen content (`protocol/PROTOCOL.md`).
- Rejected: the keys in the Swift client. Agent knowledge would live outside the adapter, and each client would need its own copy.
