# 0011: The branch's commits in Changes, and tabs in the Reader

## Status

Accepted. Extends ADR 0007 (changes) and ADR 0010 (images).

## Context

- Changes listed only what differs from HEAD (ADR 0007). An agent that commits as it works leaves nothing uncommitted. The section then disappeared from the Reader without a word, and it looked as if the diff view was gone.
- The Reader's sidebar was one list: Changes, Plan, the Markdown folders, then Images. In a repo with many Markdown files, Images were a long scroll away.

## Decision

- **`change-list` also carries `branch`: what the branch's own commits changed since it left its base**, behind the capability `branch-changes`.
  - The base is `origin/HEAD`. Without it, the base is the main worktree's branch when the pane is in another worktree (a task), else a local `main` or `master`. Tasks start from the same first two (`default_base`).
  - The files are `git diff $(git merge-base HEAD <base>) HEAD`. `diff` takes `scope: "branch"` to open one of them.
  - Uncommitted work stays in the uncommitted list, so a file can be in both lists.
- **The Reader's sidebar has tabs: Docs, Changes, Images.** Each tab has its own list and empty state, and search filters the open tab. The tab is remembered while the app runs. The approval card's Read Plan opens Docs.

## Consequences

- The label names the base ("Since origin/main · 3 commits"), so a wrong base is visible. A branch started from another feature branch also shows that branch's commits. A stale `origin/main` shows more than the branch changed. The bridge never fetches.
- The base rule lives twice: a blocking copy in `changes/git.rs`, and the async `default_base` in `tasks/builtin.rs`. They differ in the last step: a single checkout on a feature branch also needs a local `main` or `master`. Merging them is a follow-up.
- Opening the Reader no longer jumps to Changes when files are uncommitted. The dot on the Reader button still counts uncommitted files.
- Rejected: Images higher in the single list. That only moves the scroll to the Markdown.
