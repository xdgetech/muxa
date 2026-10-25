# 0005: Agent conversations through bridge-side adapters

## Status

Accepted.

## Context

On a phone or tablet, a coding agent is easier to follow as messages, like the Claude and Codex mobile apps, than as a terminal. The terminal screen holds only the visible rows, and parsing it into messages would be fragile. Agents record their conversations themselves, but in private formats that differ per agent and change between versions. muxa must support more than one agent without the apps or the protocol learning each format.

## Decision

- **The bridge reads the agent's own records** through an agent adapter (`crates/muxa-bridge/src/agents/`). It normalizes them into one neutral list of entries (`user`, `assistant`, `tool`, `notice`). The protocol carries that list (`chat`, `chat-entries`, capability `chat`). Clients only render it.
- **An adapter is a factory of stateful sources.** A source is polled on the blocking pool and returns patches (reset, upsert). Pairing a tool call with its result, or merging the lines of one turn, needs state. The chat task that turns patches into messages is the same for every agent.
- **Matching an agent to a pane uses recorded facts.** For Claude Code that is its `sessions/<pid>.json` file:
  - the pane id **and** the session name match, because pane ids restart with the tmux server and the file names no socket;
  - the recorded pid runs inside the pane, checked with `ps`, because files of exited instances remain;
  - the instance is interactive.

  Otherwise the answer is `unsupported`, and the client shows the terminal.
- **Plug-ins, later:** an external adapter is an executable set explicitly with `--agent-exec NAME=PATH`. It would be one more source kind. The bridge never searches `PATH` for adapters.
- **Transcripts are treated like pane bytes:** never logged, sent only over the SSH channel.

## Consequences

- When Claude Code changes its format, the chat degrades. Unknown lines are skipped, and the terminal stays available. A checked-in sample (`crates/muxa-bridge/tests/data/claude-code/`) pins the shape the adapter expects.
- Codex or another agent is one more file in `agents/` plus samples and tests. The protocol and the apps do not change.
- Reading files under the user's home grants nothing new: the same SSH key can already type into the panes. The directories come from the bridge's command line, which the forced command sets, not from the client.
- The chat may show messages that Claude Code itself hides after `/compact` or a rewind, because only the file's tail is read, not the conversation tree.
- A pending permission prompt is not in the transcript. Clients learn of it from the `waiting` status and must not send Enter blindly.
