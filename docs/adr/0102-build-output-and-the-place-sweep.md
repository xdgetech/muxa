# 0102: Build output on a build disk, and the place sweep

Status: proposed (2026-11-26).

## Context

- On a Mac mini host, muxa's Tasks used more than 100G of the internal disk. The host has an external SSD, and Xcode's DerivedData, Gradle, the Android emulators and the package caches already live there.
- Each Task worktree builds into its own `target/` (4 to 11G) and `apple/MuxaKit/.build` (about 1.4G), on the internal disk.
- ADR 0071 removed the Task records, and with them ADR 0043's sweep. Since then a Task's worktree goes only on Approve or Close Task. A Task whose agent terminal was removed another way leaves its worktree with no owner. Nothing records which places muxa made: `muxa.places` lives only on the agent terminal.
- Claude Code's subagents with worktree isolation make worktrees under `<repo>/.claude/worktrees`. Claude Code removes one only when the subagent changed nothing. On the host, 27 of them used 65G.
- iOS simulators held 62G. muxa does not make them; `make test-app` and Xcode do.

## Decision

1. **Build output goes to a build disk, one folder per place.**
   - `workspaces.toml` takes a host default and a per-Workspace setting. Both are file-only, like `[codex]` (ADR 0074):

     ```toml
     [build]
     dir = "/Volumes/Atlas/muxa/build"   # absolute
     drop_after = "3d"                   # see 3

     [[workspace]]
     name = "xdge"
     [workspace.build]
     dir = "/Volumes/Other/build"        # optional; overrides [build] dir
     paths = ["target", "apple/MuxaKit/.build"]   # default ["target"]
     ```

   - When a Task starts in a branch worktree, each of `paths` becomes a symlink to `<dir>/<workspace>/<worktree folder name>/<path, '/' as '-'>`. A Task folder gets no links: it is not a repo.
   - Each link is added to the repo's `info/exclude` as `/<path>`. A `target/` rule in `.gitignore` matches a folder, not a link, so without this every worktree would look dirty.
   - A symlink, not an environment variable: SwiftPM has no variable for its build path, and the agent, the Task's shells and the user all build through the same link.
   - One folder per place, not one shared target dir. Parallel Tasks do not wait on Cargo's lock, and the folder goes with its place.
   - When `dir` does not exist (the disk is not mounted), muxa makes no link and logs it. The place builds in place. muxa never creates a folder under `/Volumes`.
   - A real folder already at a path stays. The disk report lists it.
2. **muxa records the places it made.** `<state dir>/places.json` lists each Task worktree and folder: the Workspace, kind, path, branch, build folders and the time. This brings back ADR 0043 §5's list for places only. It does not bring back Task history (ADR 0071, not decided #2). The first sweep adds a record for each place named by the `muxa.places` label of a terminal herdr-core still lists.
3. **The daemon sweeps recorded places.** At start and every 10 minutes, when herdr-core answers:
   - A place is held while a live terminal names it in `muxa.places` or `muxa.dir`, or while an ended agent names it and an open Inbox item names that agent (its Review or Failed item). herdr-core lists only live terminals, and Retry and Approve need an ended agent's places until the user decides.
   - A place made in the last 10 minutes is left alone: a starting Task has its worktree before its agent terminal, and a new branch counts as merged.
   - An unheld branch worktree that is clean and merged goes, with its branch (`git branch -d`) and its build folders. The checks are those of Close Task (ADR 0072).
   - An unheld worktree that is dirty or unmerged stays. The Inbox gets one Decide item that says why and how big it is. muxa never removes it without the user.
   - An unheld Task folder stays (it holds the user's files). Its build folders go.
   - The build folders of a place held only by an ended agent are emptied after `drop_after`. They stay as empty folders, so the links still work. Build output can be made again, so no work is lost.
   - When herdr-core cannot list its terminals, the sweep does nothing: no place can be proven unheld.
   - A worktree with no record is never touched.
4. **Claude Code's worktrees are listed, not blocked.** Claude Code picks where they go, and isolation is useful. The disk report lists `<repo>/.claude/worktrees/*` with their state. `muxa-cli disk clean` removes the clean and merged ones when the user asks. The sweep never touches them, because muxa did not make them (ADR 0043 rejected touching other tools' worktrees).
5. **`muxa-cli disk` reports and cleans, through protocol v2.**
   - `disk` returns rows: recorded places, worktrees in `.worktrees` with no record, Claude Code's worktrees, Task folders nothing holds, and build folders with no place. Each row has its size, build size, holder, clean and merged.
   - `disk-clean` is a dry run unless asked. Without `force` it removes only unheld rows that are clean and merged, or build folders. A worktree muxa did not make must also be unchanged for a day, since a Claude Code subagent may be using it. `force` applies only to the paths the user names. Held places are never removed. A Task folder goes only with `force`.
   - A client names only paths from the daemon's own list (invariant 3).
6. **Simulators and tool caches are documented, not handled.** `CONTRIBUTING.md` says how to free them and what already lives on the build disk.

## Consequences

- A Task's build output costs internal disk only when the build disk is missing.
- `cargo clean` replaces the `target` link with a real folder. The disk report shows it.
- A Task ended without Approve no longer leaves its worktree for ever: it goes when safe, or it asks.
- The sweep adds a periodic `git status` and `git merge-base` per unheld place.
- A Task that ended without Approve and whose Inbox item is Done loses its worktree on the next sweep when nothing would be lost. Retry then makes a new one.

## Alternatives not taken

- **`CARGO_TARGET_DIR` for the agent:** covers Cargo only, not the Task's shells or SwiftPM.
- **One shared target dir:** parallel builds wait on one lock, and its output never goes.
- **Sweep every worktree in `.worktrees`:** the user's own worktrees live there too.
- **Stop Claude Code from making worktrees:** muxa cannot set where they go, and blocking isolation costs the agent more than the disk.
