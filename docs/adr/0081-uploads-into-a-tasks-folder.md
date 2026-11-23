# 0081: Files from any device go into the Task's `.muxa/uploads/`

## Status

Proposed (2026-11-23). Wire: `protocol/PROTOCOL-v2.md`, `upload`.

## Context

The developer wants to give a Task's agent a screenshot or a log from whichever device they hold: a crash log on the Mac, a photo of a screen on the phone. Until now a file reached the host only through some other tool.

## Decision

1. **`upload` copies one file into the Task's working place, under `.muxa/uploads/`.**
   - `.muxa/` is already excluded from git in a worktree, so an upload never lands in a commit.
   - The agent and the Task's shells work in that place, so a relative path reaches the file.
2. **The server checks the request and the host writes the file**, as with the Reader. The server never touches files.
   - The name is a file name only: no folder, nothing hidden, no control character.
   - The file is at most 10 MB, sent as base64 in one control frame (16 MiB).
   - The host never replaces a file: a taken name gets a number. It refuses an uploads folder that a symlink leads out of the working place.
3. **The apps type the path into the agent, without Enter.** The user sends it on with their words, so muxa never sends anything on its own. Claude Code reads an image from a path.
   - On iOS: a photo or a file from the Task screen's paperclip button. On Android: a file.
   - On the Mac: a file from the pane's button, or dropped on a pane.
4. **The bytes never reach a log.** The host logs the terminal and the size. A malformed `upload` is refused without quoting its values.

## Consequences

- Files are not removed by muxa. They go with the Task's folder, or with its worktree on Approve.
- A file over 10 MB needs another way, such as `scp`.
- Alternatives not taken:
  - Chunked uploads for any size: more protocol for a case the developer did not ask for.
  - A shared uploads folder per Workspace: the agent of one Task would see another Task's files.
  - Attach the file to the agent's input directly: each agent takes files differently, and a path works for all of them and for a shell.
