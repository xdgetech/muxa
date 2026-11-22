# Security

## Reporting

Report vulnerabilities privately through GitHub's "Report a vulnerability" (Security tab). Do not open a public issue.

## Threat model

muxa gives a remote device keyboard access to the terminals of your agents, and lets it start new ones. That is shell access. The design keeps the attack surface to what SSH already exposes.

- **Transport.** A device runs `muxa-bridge connect` as an SSH exec command; the bridge only relays bytes to the daemon's socket. Nothing opens a network port. Authentication, encryption and host identity come from SSH. The app pins the host key on first use and keeps its ed25519 key in the device Keychain, readable only when the device is unlocked.
- **Restrict the key.** An SSH key that can run the bridge can also run a shell, unless you restrict it. The app adds its public key to `~/.ssh/authorized_keys` with a forced command. The setup uses your password for one login after the host key is trusted, and does not store it. You can also add the line by hand:

  ```
  command="~/.local/bin/muxa-bridge",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 AAAA... muxa-ipad
  ```

  With that line the key can only reach the daemon: `muxa-bridge` relays when the client asks for `connect` and runs nothing else. It cannot install, stop or remove the daemon.
- **The daemon** (`muxa-bridge daemon`) is installed by the user in a shell. It listens only on `server.sock` in its directory (`~/.local/state/muxa/` by default; the directory has mode 0700, the socket 0600), and closes a connection from any other user, root included.
- **Input from the client is untrusted.**
  - Frames have a size cap; a malformed frame ends the connection.
  - Terminals are named by herdr-core's ids. muxa acts only on the terminals it started, which it finds by their `muxa.*` labels, besides the input the user types.
  - A Task's places: a repo is absolute, or relative to the Workspace's root and must stay inside it after symlinks are resolved, unless `workspaces.toml` lists it. git must accept a branch or base as a branch name, so no name is read as an option. A new folder goes only under a Workspace's root.
  - The agent's command and environment come from the host's `workspaces.toml` and the daemon's flags. Since ADR 0074 a client can edit that file through `settings-update`, unless the host runs the daemon with `--settings-read-only` (see Settings below). A Task's start still takes no command from the client: the agent gets one fixed prompt, and no client text reaches a command line.
- **Terminal content is untrusted** and is passed through as data. Nothing logs terminal bytes. Logs carry ids, paths, sizes and errors only.
- **The Reader only opens what the daemon listed itself** (`protocol/PROTOCOL-v2.md`, Reader). A client names a path from that list, never a path of its own, and the file must stay inside the terminal's folder, also through a symlink. The content is never logged.
  - **Documents:** the repo's Markdown files that git does not ignore, or outside a repo every Markdown file in the folder, and `.muxa/TASK.md`, `HANDOFF.md` and `CHECK.md`. HTML only when asked. At most 2 MiB each. Names that start with a dot are never listed otherwise.
  - **Images:** PNG, JPEG, GIF, WebP and HEIC files, including folders git ignores, such as QA screenshots in `build/`. Only these extensions are listed, so an ignored `.env` or key file never is. SVG is left out because it can carry scripts. At most 11 MiB each. An image in an ignored folder can be private.
  - **Changes:** the repo's uncommitted changes, and the files the branch's own commits changed since its base. A diff is at most 2 MiB. git runs read-only, and the daemon never fetches for it.

  The apps render documents and diffs offline in a web view with a content security policy that allows no network, and sanitize their HTML. HTML documents run in a sandboxed frame with no app bridge (ADR 0065).
- **Agent environment and connections** (ADR 0016, 0069, 0075).
  - `[workspace.agent] env` in `workspaces.toml` gives a workspace's agents their variables. The host's file sets them, by hand or through `settings-update`. Logs carry variable names, never a value.
  - A `[[workspace.connection]]` is a secret that every agent of its workspace gets as one variable: every Task, restart and Routine run (ADR 0075). So every agent of a workspace holds all of that workspace's secrets, including Routines that read untrusted text. To keep a token from an agent, put that agent in another workspace. The secret is in the host's macOS Keychain (service `muxa`, account `<workspace>/<name>`), read with `/usr/bin/security` when an agent starts. It is never in `workspaces.toml`, a Task folder, a log or an error message. It crosses the wire only inbound, when a client sets it through `settings-update` (see Settings below), and never back. A missing or locked secret stops that agent's start, and the error names the connection only.
  - A `[[workspace.mcp]]` is an MCP server that every agent of its workspace gets. Once a workspace lists any `[[workspace.mcp]]`, Claude Code gets `.muxa/mcp.json` in its working place with all of them and no others, and `--strict-mcp-config`, so servers from the user's or the project's config do not load. A workspace without any leaves the agent's own MCP config alone. The file holds commands, URLs and plain variables from `workspaces.toml`, never a secret: a server's connections go into the agent's environment, and the file names them as `${VAR}`. Do not put a secret in a server's `env` or `headers`; it would be written to that file. Codex and other agents get no MCP servers; a Codex agent in a workspace with servers is refused before it starts.
  - `workspaces` lists, under each connection and MCP server, every Routine and live Task agent of the workspace, and when an agent last started with it. `<state dir>/usage.json` keeps those times, with names only, never a value.
  - claude.ai connectors follow the agent's Claude login, not muxa (ADR 0069). Whether `--strict-mcp-config` hides them is not verified.
  - This is less ambient access, not a boundary. Every agent runs as the same user: one with a shell can still read other processes' environments, the daemon's socket and, after a prompt, the Keychain.
- **Check commands run on the host** (ADR 0019). A Task Loop runs its check with `/bin/sh -c`, in the Task's working place, as your user, with no one watching. Its environment is the daemon's, plus the workspace's `[workspace.agent] env`.
  - Only `workspaces.toml` allows checks, per workspace, in `[workspace.loop]`. A client can change that table through `settings-update` unless the host locks settings. Without that table, the workspace has no loops. With a list, a client can only pick one of its commands, exactly. With `"any"`, a client can send any command of at most 4096 bytes. Prefer a list.
  - A check's output goes to `.muxa/checks/` and `.muxa/CHECK.md`, never to a log. The loop types only a fixed prompt, and never answers an agent's permission, question or plan.
- **Routines** (ADR 0026). `[workspace.routines]` in `workspaces.toml` allows a workspace's Routines. A run's agent has the same access as any agent in that workspace; prefer a `worker` with narrow permissions. The daemon checks `.muxa/routine/output.json` in its own process before it stores anything: it refuses a symlink, a file that is not regular, control characters, oversize fields and unknown fields (`docs/agents.md`, Routine output). Logs carry ids, counts and outcomes, never instructions, titles, bodies, links or state.
- **Approvals.** The approval card in Chat offers only keys the adapter checked against the agent's menu (ADR 0009); muxa never picks one by itself.
- **Agent transcripts** (for Chat) are read from the agent's own files under your home directory: `~/.claude` for Claude Code. They go only over the same SSH channel, and are never logged. Where the daemon looks is fixed by its flags (`--agent-dir`) and the agent directories in `workspaces.toml`, not by the client.
- **Settings from the apps** (ADR 0074). `settings` shows `workspaces.toml`, each `.muxa/routines.toml` and the Routines' instruction files; `settings-update` edits them.
  - The view never carries an env value or a secret: env keys show as `set`, a connection's secret as `set`, `missing`, `locked` or `unavailable`.
  - An env value and a secret cross the wire only inbound, inside the SSH channel. They are never echoed, logged or quoted in an error. An env value goes only into `workspaces.toml`, as a hand edit would put it. A secret goes only to the Keychain, hex-encoded on the stdin of `/usr/bin/security -i`, never on a command line, at most 1800 bytes.
  - The daemon applies the ops only when the result passes the same checks as loading the files, and only when the files have not changed since the client read them (`revision`). It writes each file atomically, keeps its mode and one `.bak` beside it. An instruction file must stay inside `.muxa/`, also through a symlink.
  - **Lock it on the host:** `muxa-bridge daemon install --settings-read-only` (or `daemon run --settings-read-only`) makes every `settings-update` fail with `read-only`. The apps can still read the settings.
- **What a stolen client key gains.** The key could already type into every agent's terminal, and an agent with a shell runs any command. So it was already equal to a shell as your user. With `"any"` checks, it can also have a command run with no one watching; with a list, only the listed commands. Since ADR 0074 the key can also change a Workspace's agent command and environment and allow any check through `settings-update`, so the check list no longer limits a stolen key, unless the host runs with `--settings-read-only`. It can also replace a connection's secret, but never read one. To cut a device off, remove its line from `~/.ssh/authorized_keys`.
- **Stop everything.** On the host, `muxa-bridge daemon stop` stops the daemon until the next login, and `muxa-bridge daemon uninstall` also removes its service file. The agents keep running in herdr-core; end them there.
- **Out of scope:** an attacker who already has a shell as your user, and a compromised SSH server or herdr-core.
