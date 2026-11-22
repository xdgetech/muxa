@AGENTS.md

## Claude Code notes

- The project hook in `.claude/settings.json` blocks Bash `tmux` commands that reach the default server, where older agents may still run. A block is intended. Do not rephrase the command to get past the hook.
- Tests use the fake backend. Never run anything against the developer's herdr-core; ask the user to run `make test-herdr` themselves.
- Before claiming a change is done, run the checks listed for it under Verification in `docs/plan.md` and report the actual output.
