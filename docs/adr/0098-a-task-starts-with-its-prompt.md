# 0098: A Task's agent starts with its prompt

Status: accepted (2026-11-25). It replaces the typed start prompt of ADR 0015 for protocol v2.

## Context

- A v2 `start-task` writes `.muxa/TASK.md` and runs the agent, but sends no message. The agent waits at an empty prompt.
- The user: "creating task from inbox item doesnt automatically start the thinking. I need to explicitly say what needs to be done again in the agent". A Bot that starts Tasks with `muxa-cli v2 start-task` had to type the prompt into each one.
- v1 typed `Read .muxa/TASK.md and start.` once the adapter reported `idle`, within 30 s (ADR 0015). The wait had to outlive a CLI client that exits after the reply (ADR 0017).
- Routine runs already give the agent its first prompt as the last argument (`src/v2/runner.rs`).

## Decision

1. **The start prompt is the agent's last argument.** The daemon puts it on the launch line before the terminal starts. Nothing is typed and nothing waits, so it works the same from every app and from a CLI that exits right after the reply.
2. **The prompt** points at `.muxa/TASK.md`:

   | Task | Prompt |
   |---|---|
   | new | `Read .muxa/TASK.md and do the Task.` |
   | new, plan mode | `Read .muxa/TASK.md and plan the Task.` |
   | `existing` or `checkout` place, or an ended agent that restarts | `Read .muxa/TASK.md and continue the Task.` |
   | `existing` or `checkout`, plan mode | `Read .muxa/TASK.md and plan how to continue the Task.` |

   A Task from an Inbox item is a new Task: its proposal is the goal in `TASK.md` (ADR 0082).
3. **A talk with a Bot gets none.** It waits for the user (ADR 0083).
4. **Only a new terminal gets it.** When the Task's agent already runs, `start-task` returns it with `existing: true` and nothing is sent. The prompt is never sent twice.
5. **Claude Code gets `-- '<prompt>'`.** `--mcp-config` and `--add-dir` take a list, so without `--` the prompt becomes one more item (`docs/findings/claude-code-input.md`). Codex gets `'<prompt>'` (`codex [OPTIONS] [PROMPT]`). Any other program gets no prompt: it may not take one.

## Consequences

- A Task starts working with no typing, from the Inbox, Capture, the apps and `muxa-cli v2 start-task`.
- `--continue` with a prompt adds the prompt as a new message to the kept conversation.
- An agent that is neither Claude Code nor Codex still opens at an empty prompt. No note says so.

## Alternatives not taken

- **Type the prompt once the agent reports idle (ADR 0015).** It needs a wait that the daemon owns, a timeout, a note when the time runs out, and a guard against typing twice. Claude Code reports `working` and `done` with OSC 7501, but no state for a new session was checked. The argument has none of these problems.
- **`--append-system-prompt`, as a talk does.** It tells the agent what to do, but the agent still waits for a first message.
