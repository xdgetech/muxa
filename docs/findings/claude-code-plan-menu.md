# Claude Code's plan menu: verified facts

A Task from an approved proposal starts Claude Code in plan mode, and the Inbox approves its plan (ADR 0060 §4, §5). `items/answer/prompt.rs` (`approve_plan`) and the `plan` item's answers rely on these facts. Checked on Claude Code 2.1.322 (`claude --version`), macOS. Rows marked *probe* come from `scripts/repro/claude-plan-menu.sh` (run with `MUXA_REPRO_CLAUDE=1`): a throwaway `CLAUDE_CONFIG_DIR`, a dummy API key, and a fake Messages API on 127.0.0.1 that answers with a `Write` of the plan file, then `ExitPlanMode`. No model and no login run. Rows marked *binary* come from the strings in the 2.1.322 executable. Re-run the script after a Claude Code upgrade.

| Fact | Source |
|---|---|
| `claude --permission-mode plan` starts in plan mode: the footer says "⏸ plan mode on". | *probe* |
| `ExitPlanMode` needs the plan file first: "No plan found. Please write your plan to the plan file first." The file is `$CLAUDE_CONFIG_DIR/plans/<slug>.md`, and the request names its path. | *probe*, *binary* |
| The menu: "Ready to code?", the plan, "Would you like to proceed?", then numbered options. In this setup: `❯ 1. Yes, and use auto mode`, `2. Yes, manually approve edits`, `3. Tell Claude what to change` with "shift+tab to approve with this feedback". The cursor starts on 1. | *probe* |
| Other options exist and depend on settings, the account and the context: "Yes, clear context" with " and auto-accept edits", " and bypass permissions" or " and use auto mode", "Yes, auto-accept edits", "No, keep planning", "No, refine with Ultraplan in a cloud session". So the number of "Yes, manually approve edits" is not fixed, and `1` approves with a mode that differs by setup. | *binary* |
| Pressing the option's digit picks it at once, without Enter, and the menu closes. | *probe* |
| After "Yes, manually approve edits" the footer says "⏸ manual mode on", and the next `Write` asks first ("Do you want to create probe.txt?"). | *probe* |
| `Escape` on the menu closes it and ends the turn: the input box is empty and ready, the session file's `status` is `idle`, and the mode stays "plan mode on". Text typed next is a new prompt, and the model gets it. | *probe* |
| `capture-pane -p -J` shows each option on its own line as `<n>. <label>`, with `❯ ` before the focused one. | *probe* |

## What muxa does with it

`approve` on a `plan` item never guesses a key. While the adapter still reports the `ExitPlanMode` call the item asked about, the host asks muxa-tmux whether the screen shows `<n>. Yes, manually approve edits` for each `n` from 1 to 9. Only when exactly one `n` matches does it press that digit. Otherwise the answer is `conflict`, and the user approves in the terminal. Only yes or no leaves muxa-tmux; no screen content is kept or logged. A plan whose text holds the same numbered line could match too; the adapter's check that the menu is the one asked about is the guard against a stale screen.

`reject` (Keep planning) presses `Escape`. `reply` presses `Escape`, waits for the turn to end, and sends the text as the next prompt, as on a permission (`docs/agents.md`, Claude Code).
