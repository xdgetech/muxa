# Claude Code's resume flags and the root Ask session: verified facts

The overall assistant's chat is the root Ask session (ADR 0054). When its agent starts again, it continues the same conversation (ADR 0056). These facts say what muxa does and what Claude Code does with its resume flags. Checked on Claude Code 2.1.322 (`claude --version`), macOS. Rows marked *probe* come from `scripts/repro/claude-continue.sh` (run with `MUXA_REPRO_CLAUDE=1`; a throwaway `CLAUDE_CONFIG_DIR`, never `~/.claude.json`, nothing typed, so no turn runs). Re-run the script after a Claude Code upgrade.

| Fact | Source |
|---|---|
| muxa starts a root Ask session with the workspace's agent command as written (`tasks/ask.rs`, `start_root` through `Builtin::start`). The secretary's session (ADR 0056) is the exception: muxa appends `--resume <id>` when it knows the conversation's id and the command starts `claude` (`secretary/command.rs`). | code |
| muxa reads the id from Claude Code's `sessions/<pid>.json` for the pane, through the adapter, only when the file's process runs in the pane (`process_bound`). It keeps it in `<state dir>/assistant/secretary.json`. | code |
| `claude --continue` continues the most recent conversation in the current folder. | `claude --help` |
| `claude --continue` in a folder with no earlier conversation exits at once with status 1 and prints "No conversation found to continue". The pane is dead before any prompt. | *probe* |
| `claude --resume <id>` with an id Claude Code does not know exits at once with status 1 and prints "No conversation found with session ID: `<id>`". | *probe* |
| `claude --resume <id>` with an id of a conversation in `projects/<escaped root>/<id>.jsonl` stays open and shows that conversation. | *probe* |
| The resumed instance's `sessions/<pid>.json` has `sessionId` = the resumed id. muxa sees the same id again, so one id serves every restart. | *probe* |

## What this means for muxa

- A root Ask session cannot always start with `--continue`: the first start in a new root would exit.
- "Most recent conversation in the folder" includes one the user started by hand in the root. Resuming by the conversation's id avoids that. muxa keeps the id in its own state file, because an ask session has no record (ADR 0031).
- A wrong id costs one start: the agent exits at once. The daemon treats a resumed start that ends within 30 s as a failed resume, drops the id and starts fresh once (ADR 0056 §3).
- Only the secretary's session resumes. Other root Ask sessions start fresh.
