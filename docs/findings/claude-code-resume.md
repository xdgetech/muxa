# Claude Code's resume flags and the root Ask session: verified facts

The overall assistant's chat is the root Ask session (ADR 0054). When that session ends, the next one should continue the same conversation. These facts say what muxa does today and what Claude Code does with its resume flags. Checked on Claude Code 2.1.322 (`claude --version`), macOS. Rows marked *probe* come from `scripts/repro/claude-continue.sh` (run with `MUXA_REPRO_CLAUDE=1`; a throwaway `CLAUDE_CONFIG_DIR`, never `~/.claude.json`, nothing typed, so no turn runs). Re-run the script after a Claude Code upgrade.

| Fact | Source |
|---|---|
| muxa starts a root Ask session with the workspace's agent command as written, with no resume flag (`tasks/ask.rs`, `start_root` through `Builtin::start`). A restarted session starts a new conversation. The old one stays in Claude Code's own history. | code |
| `claude --continue` continues the most recent conversation in the current folder. | `claude --help` |
| `claude --continue` in a folder with no earlier conversation exits at once with status 1 and prints "No conversation found to continue". The pane is dead before any prompt. | *probe* |
| `claude --resume <id>` with an id Claude Code does not know exits at once with status 1 and prints "No conversation found with session ID: `<id>`". | *probe* |

## What this means for muxa

- A root Ask session cannot always start with `--continue`: the first start in a new root would exit.
- "Most recent conversation in the folder" includes one the user started by hand in the root. Resuming by the conversation's id avoids that, but muxa must then remember the id. An ask session has no record to keep it (ADR 0031).
- So muxa does not resume the root Ask session yet. The next milestone remembers the Claude Code session id of the overall workspace's root Ask session, as the Task runs do (`agentSession`, docs/agents.md), and starts the next one with `--resume <id>` only when Claude Code still has that conversation.
