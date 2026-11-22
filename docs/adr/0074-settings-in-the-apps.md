# 0074: Settings in the apps

## Status

Accepted (2026-11-22, the developer's decision). Changes ADR 0015 and ADR 0016, where only the host's file set Workspaces and agent commands. Builds on ADR 0019, 0026, 0036 and 0069. Wire: `protocol/PROTOCOL-v2.md`, Settings.

## Context

Until now a person edited the host's `workspaces.toml` and each Workspace's `.muxa/routines.toml` by hand, in a shell on the host. The apps could only read a summary (`workspaces`, `routines`). The developer decided that every setting must be editable from each app: Workspaces, agent env, loop checks, the Routines gate, connections with their secrets, MCP servers, Routines with their instructions, and workers.

## Decision

1. **The files stay the source of truth and stay hand-editable.**
   - The daemon edits them with `toml_edit`. Keys keep their order and comments, and a value is rewritten only when it changes.
   - After an edit, the daemon prints the document, reads it back with `toml`, and compares the values. A table printed under the wrong `[[workspace]]` would change the meaning, so the update is refused instead.
   - Each file is written whole to a temporary file beside it, then renamed. The old file is kept as `<file>.bak`. A file keeps its mode; a new file gets 0600, because it can hold env values. A symlinked `workspaces.toml` (a dotfiles checkout) is written at its target.
   - Nothing caches the files: the daemon already reads them on each use.
2. **`settings` returns the files as written**, read leniently, so a file the loader refuses still shows, with the reason (`problem`, `routines_problem`). Paths keep `~/`. A Workspace's Routines show while its gate is off.
3. **A revision guards hand edits.** `revision` is a SHA-256 over the path and bytes of `workspaces.toml`, every routines file and every instruction file. `settings-update` with another revision is `conflict`, and nothing changes.
4. **Ops are granular, ordered, and all or none.**
   - Each op names one thing: a Workspace, an env key, the loop checks, the Routines gate, a connection, an MCP server, a Routine or a worker. Update ops replace that thing's fields; for `workspace-update`, a missing field keeps its value and `null` removes it.
   - After each op, the result must pass the loader's own rules (`Workspaces::parse`, `routines::parse_text`). There is no second set of rules to drift. The Routines of the op's Workspace must load, even with the gate off, and Routines that loaded before must still load. So removing a connection a Routine names fails until the Routine lets it go.
   - A failure names the op (`ops[2] connection-add: …`) and writes nothing.
   - Names are not renamed. A Keychain account, a Routine's state and a Task's labels hold them; a rename is a remove and an add.
5. **Write-only values.** An env `value` and a connection `secret` travel only to the daemon, inside the SSH channel. The view shows env keys with `set: true` and a connection's `secret` as `set`, `missing`, `locked` or `unavailable`. `WriteOnly` redacts them in `Debug`; the Swift and Kotlin op types redact them too. A malformed `settings-update` is answered without serde's quote of a value.
6. **Secrets go through `/usr/bin/security`, as reading does.**
   - An item made by the framework API would trust `muxa-bridge`, and then `security find-generic-password`, which starts agents, would prompt. An item made by `security` has the same access list as one added by hand.
   - The secret goes hex-encoded (`-X`) on the stdin of `security -i`, never on a command line. `security -i` reads lines of about 4 KiB and echoes a split line in its error, so a secret is at most 1800 bytes, and its output is discarded. It exits 0 even when a command failed, so the daemon reads the secret back to confirm it.
   - The state check never reads a secret: `show-keychain-info` tells locked, and `find-generic-password` without `-w` tells set (0) or missing (44).
   - The Keychain is written before the files, so a refused secret writes no file. A file write that fails after it can leave a new secret without its connection. A removed connection's secret, and those of a removed Workspace, are deleted after the files are written.
   - Tests use the `Secrets` trait's fake and never the developer's Keychain.
7. **The host can lock it.** `muxa-bridge daemon run|install --settings-read-only` makes `settings-update` answer the new error code `read-only`. The view says `read_only`.
8. **Out of scope.** `[codex]` stays a hand edit. A removed Workspace's folder and `.muxa/`, and a removed Routine's instruction file, stay on disk.

## Consequences

- A device key can now change a Workspace's agent command, its env, and its loop checks, including `"any"`. That was already equal to a shell as the user (SECURITY.md, What a stolen client key gains), but the check allowlist no longer limits a stolen key unless the host runs with `--settings-read-only`.
- A person editing a file by hand while an app edits it gets `conflict` in the app, never a silent overwrite. A hand edit between the daemon's revision check and its write is not detected; the window is a few milliseconds.
- `muxa-cli v2 settings` and `settings-set` script the same ops.
- The demo servers and the fake server keep settings in memory with the same names and error codes, but check no paths or schedules.

## Alternatives

- **A settings database owned by the daemon.** Rejected: the files are reviewed in git and edited by hand, and agents can write `.muxa/`.
- **Whole-file upload.** Rejected: it would carry env values back to the device and could not keep secrets out of the view.
- **`security-framework`.** Rejected for the access-list reason in 6.
