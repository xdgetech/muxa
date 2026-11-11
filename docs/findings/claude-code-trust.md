# Claude Code's folder trust: verified facts

Assistant runs, plan runs and Routine runs start Claude Code in a new folder each time. When it shows its trust dialog there, the start prompt is never typed. `agents/claude/trust.rs` and the failure reasons rely on these facts. Checked on Claude Code 2.1.322 (`claude --version`), macOS. Rows marked *probe* come from `scripts/repro/claude-trust.sh` (run with `MUXA_REPRO_CLAUDE=1`; a throwaway `CLAUDE_CONFIG_DIR`, never `~/.claude.json`, no login). Rows marked *binary* come from the messages in the 2.1.322 executable and `claude --help`. Re-run the script after a Claude Code upgrade.

| Fact | Source |
|---|---|
| Interactive `claude` in a folder that neither it nor a parent was trusted in shows the trust dialog first: "Quick safety check: Is this a project you created or one you trust? …", then the options "No, exit" (focused) and "Yes, I trust this folder". It waits there. No hook runs and no state is reported, so an adapter sees no finished turn. | *probe* |
| At 80 columns, "Yes, I trust this folder" is on one line. The question above it is wrapped by Claude Code itself, so `capture-pane -J` does not join it. muxa matches the option's label only. | *probe* |
| In a subfolder of a trusted folder, no dialog shows. Trusting a workspace's root trusts every run folder made in it. | *probe* |
| The trust record is `projects["<folder>"].hasTrustDialogAccepted: true` in `$CLAUDE_CONFIG_DIR/.claude.json`, else `~/.claude.json`. Claude Code's own error text tells the user to set that key or to accept the dialog once. The walk up from the folder stops at the folder's git root. The home folder's trust is never recorded. `CLAUDE_CODE_SANDBOXED` and `CLAUDE_CODE_IGNORE_PERSISTED_TRUST` change the result. | *binary* |
| No flag of the interactive command skips the dialog. `-p` skips it but is not interactive. `--trust-workspace` belongs to a subcommand, not to `claude` itself. | *binary* (`claude --help`) |

## What muxa does with it

muxa never writes Claude Code's config. It does not decide trust before a run either: copying the rule above (git roots, the home folder, two environment variables, and a config folder the pane's shell may set) could refuse a run that would work. Instead, when a start prompt's 30 s wait ends, the daemon asks muxa-tmux whether the agent's screen shows "Yes, I trust this folder" (ADR 0048, addendum 2026-11-11). Only that yes or no leaves muxa-tmux; no screen content is kept or logged.
