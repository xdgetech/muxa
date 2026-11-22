# 0075: Every agent of a Workspace gets all its connections and MCP servers

## Status

Accepted (2026-11-22, the developer's decision). Reverses ADR 0069 §4 and §5 and the per-Task and per-Routine naming of its E2b addendum. Keeps ADR 0069 §1–§3 and §6–§8, and the addendum's §2–§5 about the MCP file, strict mode and Codex.

## Context

ADR 0069 gave an agent only the connections and MCP servers its Task (`start-task {connections, mcp}`) or Routine (`connections`, `mcp` in `routines.toml`) named, plus those with `default = true`. In use, the choice cost more than it gave:

- Every Start Task sheet and every Routine needed a picker. A forgotten choice failed a run, often hours later in a Routine.
- Removing a connection was blocked by every Routine that named it.
- §1 already said it was not a boundary. Every agent runs as the same user and can read the Keychain after a prompt.

## Decision

1. **The Workspace is the unit.** Every agent of a Workspace gets every `[[workspace.connection]]` as its variable and every `[[workspace.mcp]]` server. Agents are Tasks, restarts and Routine runs. A restart gets the Workspace's list as it is now, not the list on the ended agent's labels.
2. **Old keys are read and ignored, never refused.** Each of these is accepted, ignored, and logged once per daemon:
   - `default` in `[[workspace.connection]]` and `[[workspace.mcp]]`;
   - `connections` and `mcp` in `routines.toml`;
   - `connections` and `mcp` in `start-task`.

   An unknown name in them is not an error, so a Routine never hides or blocks anything.
3. **Strict MCP stays.** A Claude Code agent in a Workspace that lists any `[[workspace.mcp]]` gets `.muxa/mcp.json` with all of them and `--strict-mcp-config`. A Workspace with none gets no MCP flags. A Codex agent in a Workspace with servers is still refused before anything is made.
4. **Wire.**
   - `start-task`'s `connections` and `mcp` are deprecated and ignored, but still accepted.
   - In `workspaces`, `default` stays on each connection and server and is always `true`, so older clients keep decoding. `used_by.tasks` is every live Task agent of the Workspace, and `used_by.routines` every Routine in its `routines.toml`. `last_used` stays.
   - The labels `muxa.connections` and `muxa.mcp` list everything the agent got.
   - The Settings types of ADR 0074 have no `default` and no per-Routine `connections` or `mcp`. An edit of a connection, server or Routine removes those keys from the file.
   - `muxa-cli v2 start-task --connection` and `--mcp` print a deprecation note and send nothing.

## Consequences

- Wider exposure. A token in a Workspace reaches every agent of that Workspace, including Routines that read untrusted text. To keep a token from an agent, put that agent in another Workspace.
- Removing a connection or server never fails because of a Routine.
- The apps' Start Task pickers for connections and MCP servers become dead weight; the UI agents remove them later.
- Usage times in `<state dir>/usage.json` now record every connection and server at each start.

## Alternatives

- **Keep the choice and add better pickers.** Rejected by the developer: the failure mode, a run missing a token, is worse than the gain, given §1 of ADR 0069.
- **Refuse the old keys.** Rejected: existing `routines.toml` files, often written by agents, would stop loading.
