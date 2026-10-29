# Security

## Reporting

Report vulnerabilities privately through GitHub's "Report a vulnerability" (Security tab). Do not open a public issue.

## Threat model

muxa gives a remote device keyboard access to your terminal sessions. That is shell access. The design keeps the attack surface to what SSH already exposes.

- **Transport:** the bridge runs as an SSH exec command. It opens no port. Authentication, encryption and host identity come from SSH. The app pins the host key on first use and stores its ed25519 key in the device Keychain, readable only when the device is unlocked.
- **Restrict the key.** An SSH key that can run the bridge can also run a shell, unless you restrict it. The app adds its public key to `~/.ssh/authorized_keys` with a forced command. The setup uses your password for one login after the host key is trusted, and does not store it. You can also add the line by hand:

  ```
  command="~/.local/bin/muxa-bridge",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 AAAA... muxa-ipad
  ```

  The key can then reach your sessions only through the bridge. The bridge sends keystrokes only to panes that already exist, and only on explicit requests.
- **Input from the client is untrusted.** Requests have a size cap. Pane and session ids are validated. Every value placed in a tmux command line goes through one quoting function, so no request can add tmux commands or expand variables.
- **Terminal content is untrusted** and is passed through as data. The bridge never logs pane bytes. Logs contain ids, sizes and errors only.
- **The Reader only opens what the bridge listed itself.** A client names a path from that list, never a path of its own. Except for the agent's plan, nothing is read outside the pane's git repo, even through a symlink. The content is never logged.
  - **Documents:** the Markdown files of the repo that git does not ignore, and the pane's agent plan. At most 2 MiB each.
  - **Images:** PNG, JPEG, GIF, WebP and HEIC files in the repo, including folders git ignores, such as QA screenshots in `build/`. Only these extensions are listed, so an ignored `.env` or key file never is. SVG is left out because it can carry scripts. At most 16 MiB each. An image in an ignored folder can be private, for example a screenshot of a login page.
  - **Changes:** the repo's uncommitted changes, and the files the branch's own commits changed since its base. A diff is at most 2 MiB. git runs read-only, and the bridge never fetches.
  
  The app renders documents and diffs offline in a web view with a content security policy that allows no network, and sanitizes their HTML.
- **Tasks write to the host.** A client can start a task: a git worktree on a branch, and a detached session running an agent. It can also close one. The client gives the repo's absolute path and the branch name. The bridge checks that the path is a git repo and that git accepts the names as branch names, so no name is read as an option. The command that starts the agent is fixed by the bridge's command line (`--task-command`, `--task-launcher`), not the client. Closing never removes a worktree with uncommitted changes, and always keeps the branch.
- **Agent transcripts** (for the chat view) are read from the agent's own files under your home directory: `~/.claude` for Claude Code, `~/.codex` for Codex. They are sent only over the same SSH channel, and never logged. Reading them needs no access beyond what the key already has: it can type into the same panes. Where the bridge looks is fixed by its command line (`--agent-dir`), which the forced command in `authorized_keys` sets, not the client.
- **Out of scope:** an attacker who already has a shell as your user, and a compromised SSH server.
