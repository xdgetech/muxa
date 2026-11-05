# 0038: muxa for tmux's tree has sections, and each one folds

## Status

Proposed. The developer chose this on 2026-11-05. It replaces ADR 0037 §3.

## Context

- ADR 0037 §3 listed Task sessions under their repo, next to that repo's other sessions. A row did not say whether it was a Task. On the host on 2026-11-05 the tree showed three repo groups. The developer could not find "the task list" or "the session list".
- Ask sessions (`@muxa_ask`, ADR 0031) had no section of their own. They listed with manual sessions, so the developer could not find the ask session they had started, or close it.
- The tree had no help. Ask (`prefix + A`), making a manual session and the close keys were only in the README.

## Decision

1. **Four sections, in this order.** Each section has a heading row (`g:<section>`) with its session count.
   - **Tasks**: sessions with `@muxa_task`, grouped by repo, then "no repo".
   - **Routines**: Routine runs, which are repo-less Tasks whose folder is `<root>/routines/...`.
   - **Ask**: sessions with `@muxa_ask`.
   - **Sessions**: every other session, grouped by repo, then "no repo". This includes tasks made without muxa (a worktree, no `@muxa_task`).
   - A section with no sessions is not shown. A "no repo" heading shows only when the section also has a repo group.
2. **A section folds.** `Enter` or `space` on its heading folds or unfolds it. The folded sections are the global option `@agents_folded`, so every sidebar shows the same folds.
   - A folded heading shows `▸`, the count, and the most urgent state of its sessions. It shows `▎` when the sidebar's own session is in it.
   - The sessions of a folded section get no jump label. `prefix + j`, `prefix + )` and `prefix + (` skip them. `prefix + a` still finds them.
3. **`?` in the tree shows every key** in a popup (`help.sh`), with the keys the options set. This includes how to make a manual session.
4. `x` on a session with no worktree asks only "close <session>?".

## Consequences

- The jump label of a session changes when a section above it folds. What the tree shows is what `prefix + j` reaches.
- Ask sessions are found and closed in one place.
- Row targets change: `r:<repo>` becomes `r:<section>:<repo>`, and there are new `g:` rows. Only the plugin reads them.
- Alternatives not taken:
  - Folding each repo group: there are more rows to manage, and the developer asked for Tasks, sessions and ask sessions.
  - Keeping a session's own section always unfolded: the jump labels in the sidebar would then differ from those `prefix + j` uses.
  - Fold state per sidebar: two sidebars would number the same session differently.
