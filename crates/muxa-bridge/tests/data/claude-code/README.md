Made-up files in the shape Claude Code 2.1.289 writes. No real conversation.

- `session.json`: `~/.claude/sessions/<pid>.json`. The tests fill in `pid`, `sessionId` and `tmux`.
- `transcript.jsonl`: `~/.claude/projects/<escaped cwd>/<session id>.jsonl`. Its `slug` names the session's plan, `~/.claude/plans/<slug>.md`.
- `waiting-permission.jsonl`, `waiting-question.jsonl`, `waiting-plan.jsonl` (2.1.310): transcripts while Claude Code waits on a permission, an AskUserQuestion and an ExitPlanMode. The session file then says `"status":"waiting"`, and the last line is the call it waits on, with its full input.

When Claude Code changes either format, update these files from a real session with the
content replaced, and note the version here.
