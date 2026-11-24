# Claude Code's typed input: verified facts

The apps type a chat message into the agent's terminal and send it with Return (`KeyBytes.message`, `NextModel.sendMessage`). These facts set how the Return goes. Checked on Claude Code 2.1.331, macOS, 2026-11-24, with `scripts/repro/claude-input.sh`: a throwaway tmux server (`-L`), bytes written with `send-keys -H`, one command per write.

| Fact | Source |
|---|---|
| A line of up to 60 bytes with `\r` at its end, in one write, is sent. A 14-byte line and a 45-byte Japanese line were sent. | *probe* |
| A line of 70 bytes or more with `\r` at its end, in one write, is not sent. Claude Code reads the write as a paste, and the `\r` becomes a new line in the prompt. 70, 80, 120 and 300 bytes behaved the same. | *probe* |
| The same text, then `\r` as a write of its own, is sent: right after the text, after 20 ms and after 50 ms. | *probe* |
| A short bracketed paste of two lines (`ESC[200~ … ESC[201~`) with `\r` in the same write is sent. | *probe* |
| Claude Code reports `done` (OSC 7501) after each turn, and the talk then waits for the next message (`session-backends.md`). A Bot's live talk is usually `done`. | by hand |

## What muxa does with it

- The apps write the message first, then `\r` alone after `KeyBytes.submitPause` (200 ms). Over the network, two writes sent back to back may reach the terminal in one read, so the pause is longer than the probe needed.
- The host does the same through `muxa_server::typing::type_message`: Request changes' comment, and the fix prompt of a Routine run and of a Task Loop.
- A Bot's page lands on a talk whatever its state, since `done` only means the turn ended.
