# 0063: Tasks mode groups Tasks by workspace, then repo

## Status

Accepted on 2026-11-15. It amends ADR 0035 §1 (Tasks mode) in both apps. Work item w-e778037d.

## Context

- Tasks mode on iOS and Android listed every Task in five need groups (ADR 0035 §1). With several workspaces and repos, Tasks of unrelated projects sat side by side.
- The desktop tree (ADR 0038) groups Tasks by repo, in byte order, with "no repo" last. It has no workspace level.
- `task-list` already sends each record's `workspace` and `repo`, and each workspace's `root`. No protocol change is needed.

## Decision

1. **Workspace, then repo.** Tasks mode lists Routines, then one group per workspace, then one group per repo inside it.
   - Workspaces and repos sort in byte order, as the desktop's `LC_ALL=C`.
   - A repo shows by its path under the workspace root, else with the home folder as `~` (ADR 0031).
   - Tasks without a repo go last, under "No repo". That heading shows only next to a real repo, as on the desktop.
2. **Need order inside a group.** Tasks keep the Inbox order inside a repo: need group, then the newest change. The row shows the agent's state, the status and the loop. A Task closed more than a day ago is left out, as before.
3. **Folds.** A tap on a workspace or a repo folds it. The device remembers the folded groups by key: `w:<workspace>` and `r:<workspace>:<repo dir>`, or `r:<workspace>:~`. The order depends only on names, so a refresh never moves a group.
4. **One rule.** `protocol/fixtures/tasks/grouping.jsonl` pins the rule. `TaskGroups.swift` and `TaskGroups.kt` must both pass it.

## Alternatives not taken

- **Need, then workspace and repo.** It keeps "what needs me" on top, but splits one project over five places. The Inbox already answers "what needs me".
- **A switch between both.** More UI and state for one list.
- **Repo only, like the desktop.** Two workspaces can hold repos of the same name, and Tasks without a repo would mix across workspaces.
- **Grouping in the bridge.** The desktop does not use it. A wire change for a view rule would bind every client to one layout.
