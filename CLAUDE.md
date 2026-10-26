@AGENTS.md

## Claude Code notes

- The project hook in `.claude/settings.json` blocks Bash `tmux` commands that reach the default server. A block is intended. Switch to an isolated server; do not rephrase the command to get past the hook.
- Long-running tmux experiments belong in a script under `scripts/repro/`, not in ad-hoc shell sessions. The result is then reusable and checked in CI.
- Before claiming a change is done, run the checks listed for it under Verification in `docs/plan.md` and report the actual output.
