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
- **Agent transcripts** (for the chat view) are read from the agent's own files under your home directory, for example `~/.claude`. They are sent only over the same SSH channel, and never logged. Reading them needs no access beyond what the key already has: it can type into the same panes. Where the bridge looks is fixed by its command line (`--agent-dir`), which the forced command in `authorized_keys` sets, not the client.
- **Out of scope:** an attacker who already has a shell as your user, and a compromised SSH server.
