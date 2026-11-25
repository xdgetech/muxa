# 0099: Ignored documents in the Reader

## Status

Accepted.

## Context

The Reader's Docs list showed tracked files and untracked files that git does not ignore. Ignored files were left out, so a secret in an ignored file could not show. Agents often write their output to ignored folders, for example a spec-qa report in `output/qa/<ticket>/<run>/report.md`. The user could not read it in the Reader. The Images list already includes ignored files.

## Decision

- **The Docs list includes ignored Markdown files**, and ignored HTML files when the client asks for HTML (ADR 0065). The entry has `ignored: true`, and the apps show "ignored" beside the name.
- **What still keeps secrets out** of the list (these rules apply to ignored files only):
  - Only `*.md`, `*.markdown`, `*.html` and `*.htm` are listed. `.env`, keys and JSON files never match.
  - No path with a name that starts with a dot: `.git/`, `.claude/`, `.env.md`, `.venv/`.
  - No path through a dependency or build folder: `node_modules`, `target`, `vendor`, `Pods`, `DerivedData`, `bower_components`, `venv`. These are heavy and are not the user's documents. git skips them through exclude pathspecs, and the bridge checks every path again.
  - No file whose name contains `secret`, `credential`, `password`, `token` or `private`, in any case.
  - The Reader stays read-only. A client names only a listed path. Each read checks again that the real file stays inside the repo and is at most 2 MiB.
- **Tracked files come first.** `MAX_DOCS` (2000) is shared. Ignored files take only the room tracked and untracked files leave.
- **No setting.** The Reader serves the user's own host to the user's own devices. A per-Workspace switch would need the settings view, the loading rules and three apps. It can come later if these rules prove too loose.

## Consequences

- The name rule is a heuristic. A Markdown file with a secret and a plain name in an ignored folder now shows in the Reader. The developer can move it, rename it or put it under a dot folder.
- Each Docs list runs `git ls-files` twice. With the exclude pathspecs, a repo with `node_modules` took about 0.15 s instead of 0.36 s for the second run.
- An older app ignores `ignored` and shows the file like any other.
