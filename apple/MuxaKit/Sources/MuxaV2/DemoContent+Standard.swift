import Foundation

extension DemoContent {
    /// Two Tasks (one waits on the user), a shell, a terminal muxa did not
    /// start, a Routine run, every Inbox kind, and three Routines.
    /// Approving `d8` needs force: its worktree holds work. `d9` came from muxa v1.
    public static let standard: DemoContent = {
        func agent(_ id: String, _ branch: String, _ state: String) -> Terminal {
            Terminal(
                id: id, name: branch, live: true, status: TerminalStatus(state: state, app: "claude-code"),
                labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.branch": branch, "muxa.workspace": "app",
                         "muxa.repo": "/demo/app", "muxa.dir": "/demo/app-\(branch)"],
                cols: 80, rows: 24)
        }
        func item(_ id: String, _ kind: ItemKind, _ title: String, note: String? = nil, workspace: String? = "app",
                  due: String? = nil, source: String, terminal: String? = nil, routine: RoutineRef? = nil,
                  place: ItemPlace? = nil) -> InboxItem {
            InboxItem(id: id, kind: kind, title: title, note: note, workspace: workspace, due: due,
                      laterUntil: nil, created: 1_764_000_000, source: source, terminal: terminal, routine: routine,
                      place: place)
        }
        func routine(_ name: String, at: String?, running: Int64? = nil, last: RoutineLast?) -> Routine {
            Routine(workspace: "app", name: name, enabled: true, at: at, timezone: at.map { _ in "Europe/Berlin" },
                    schedule: nil, nextDue: nil, runningSince: running, last: last)
        }
        let everyRoutine = ["dependency-digest", "nightly-backup", "release-notes"]
        return DemoContent(
            terminals: [
                agent("t1", "fix-login", "working"),
                agent("t2", "docs-audit", "blocked"),
                Terminal(id: "t3", name: "shell", live: true, status: nil, labels: [:], cols: 80, rows: 24),
                // A terminal muxa did not start: no name, no labels.
                Terminal(id: "t4", name: nil, live: true, status: nil, labels: [:], cols: 80, rows: 24),
                Terminal(
                    id: "t5", name: nil, live: true, status: TerminalStatus(state: "working", app: "claude-code"),
                    labels: ["muxa.kind": "routine", "muxa.role": "agent", "muxa.workspace": "app",
                             "muxa.routine": "dependency-digest", "muxa.dir": "/demo/runs/dependency-digest"],
                    cols: 80, rows: 24),
            ],
            screens: [
                "t1": "● Reading src/auth/session.ts\r\n● Editing src/auth/session.ts\r\n  The session now refreshes before it expires.\r\n",
                "t2": "● The README links three pages that moved.\r\n\r\n  Allow edits to README.md? (y/n) ",
                "t3": "$ ",
                "t4": "% ",
                "t5": "● Reading the lockfile\r\n",
            ],
            inbox: [
                item("d1", .review, "Review fix-login", note: "Draft PR #42: refresh the session before it expires.", source: "task",
                     terminal: "t1"),
                item("d2", .reply, "Reply to Anna about the launch date", workspace: "Personal", due: "2026-11-27", source: "capture"),
                item("d3", .decide, "Upgrade the database to Postgres 17?",
                     note: "Two extensions need a new version first:\n\n- `pgvector` 0.8\n- `postgis` 3.5\n\n"
                         + "**Plan:** upgrade them, then the database. See the [release notes](https://www.postgresql.org/docs/17/release-17.html).",
                     source: "routine"),
                item("d4", .read, "Weekly dependency digest", note: "3 updates, 1 security fix.", source: "routine"),
                item("d5", .failed, "nightly-backup failed", note: "gh login expired.", source: "routine",
                     routine: RoutineRef(workspace: "app", name: "nightly-backup")),
                item("d6", .todo, "Renew the domain", workspace: "Personal", due: "2026-12-01", source: "capture"),
                item("d7", .failed, "The agent of docs-audit stopped", note: "It exited with code 1.", source: "task",
                     terminal: "t2"),
                item("d8", .review, "Review docs-audit", note: "Two links still to check.", source: "task", terminal: "t2"),
                item("d9", .todo, "Resume: Tidy the release notes",
                     note: "From the old muxa. Resume Task starts the agent where it worked.", source: "v1",
                     place: ItemPlace(workspace: "app", path: "/demo/tasks/tidy-the-release-notes-t-1a2b3c")),
            ],
            routines: [
                // t5 runs it.
                routine("dependency-digest", at: "09:00", running: 1_764_000_000, last: RoutineLast(at: 1_764_000_000, ok: true, reason: nil, published: 1)),
                routine("nightly-backup", at: "02:00", last: RoutineLast(at: 1_764_000_000, ok: false, reason: "gh login expired.", published: nil)),
                routine("release-notes", at: nil, last: nil),
            ],
            workspaces: ["app", "Personal"],
            taskWorkspaces: [
                Workspace(name: "app", root: "/demo", repos: ["app", "docs"], agent: "claude",
                          loop: WorkspaceLoop(checks: .list(["make test", "make lint"])),
                          connections: [
                              WorkspaceConnection(name: "github", isDefault: true, usedBy: UsedBy(routines: everyRoutine),
                                                  lastUsed: 1_764_000_000),
                              WorkspaceConnection(name: "linear", isDefault: true,
                                                  usedBy: UsedBy(routines: everyRoutine), lastUsed: 1_763_990_000),
                          ],
                          mcp: [WorkspaceMcp(name: "docs", isDefault: true, usedBy: UsedBy(routines: everyRoutine),
                                             lastUsed: 1_764_000_000)],
                          branches: ["app": "main", "docs": "main"]),
                Workspace(name: "Personal", root: "/demo/personal", agent: "claude"),
            ],
            files: [
                .docs: [
                    DemoFile(entry: ReaderEntry(path: "PLAN.md"), content: ReaderContent(path: "PLAN.md", text: plan)),
                    DemoFile(entry: ReaderEntry(path: ".muxa/TASK.md"),
                             content: ReaderContent(path: ".muxa/TASK.md", text: "# fix-login\n\nRefresh the session before it expires.\n")),
                ],
                .changes: [
                    DemoFile(entry: ReaderEntry(path: "src/auth/session.ts", status: "modified", added: 2, removed: 1),
                             content: ReaderContent(path: "src/auth/session.ts", text: diff)),
                ],
                .images: [
                    DemoFile(entry: ReaderEntry(path: "screenshots/login.png", size: 70),
                             content: ReaderContent(path: "screenshots/login.png", data: pixel)),
                ],
            ],
            chats: [
                "t1": ChatUpdate(reset: true, entries: [
                    ChatEntry(id: "u1", kind: "user", text: "Refresh the session before it expires."),
                    ChatEntry(id: "a1", kind: "assistant", text: "I'll schedule the refresh **60 s** early."),
                    ChatEntry(id: "t:1", kind: "tool",
                              tool: ChatTool(name: "Edit", summary: "src/auth/session.ts", result: "Updated.")),
                ]),
                "t2": ChatUpdate(
                    reset: true,
                    entries: [
                        ChatEntry(id: "u1", kind: "user", text: "Fix the README links."),
                        ChatEntry(id: "a1", kind: "assistant", text: "Three pages moved. I'll update the links."),
                        ChatEntry(id: "t:1", kind: "tool", tool: ChatTool(name: "Edit", summary: "README.md")),
                    ],
                    prompt: ChatPrompt(entry: "t:1", kind: "permission", options: [
                        ChatOption(label: "Allow", input: "y"),
                        ChatOption(label: "Deny", input: "n", cancel: true),
                    ])),
            ],
            losesWork: ["d8": "branch docs-audit in /demo/app-docs-audit has uncommitted changes. Approve with force to remove them anyway"],
            settings: settings)
    }()

    /// The Workspaces of `workspaces` as their files say, with no secret.
    private static let settings = Settings(
        revision: "1",
        workspaces: [
            SettingsWorkspace(
                name: "app", root: "/demo", repos: ["app", "docs"], parkAfter: "2d",
                agent: SettingsAgent(command: "claude", env: [EnvKey(key: "CLAUDE_CONFIG_DIR")]),
                loop: WorkspaceLoop(checks: .list(["make test", "make lint"])), routinesEnabled: true,
                connections: [
                    SettingsConnection(spec: ConnectionSpec(name: "github", env: "GITHUB_TOKEN", account: "me@example.com",
                                                            access: "read"), secret: .set),
                    SettingsConnection(spec: ConnectionSpec(name: "linear", env: "LINEAR_API_KEY"), secret: .missing),
                ],
                mcp: [McpSpec(name: "docs", command: "npx", args: ["-y", "docs-mcp"])],
                routines: [
                    SettingsRoutine(
                        spec: RoutineSpec(name: "dependency-digest", at: "09:00", timezone: "Europe/Berlin",
                                          instructions: "routines/dependency-digest.md"),
                        instructionsText: "List the dependency updates of the week, security fixes first.\n"),
                ],
                foundRepos: ["app", "docs", "legacy"], hiddenRepos: ["legacy"]),
            SettingsWorkspace(
                name: "Personal", root: "/demo/personal", agent: SettingsAgent(command: "claude"),
                routines: [
                    SettingsRoutine(
                        spec: RoutineSpec(name: "weekly-review", at: "18:00", timezone: "Europe/Berlin",
                                          instructions: "routines/weekly-review.md"),
                        instructionsText: "Sum up the week's Done items.\n"),
                ]),
        ],
        workers: [WorkerSpec(name: "reviewer", description: "Reviews diffs", agent: "claude-code")])

    private static let plan = """
        # Plan

        1. Refresh the session 60 s before it expires.
        2. Retry once when the refresh fails.
        3. Add a test for a session that expired while the app slept.

        The goal is in [the Task's brief](.muxa/TASK.md).

        """

    private static let diff = """
        diff --git a/src/auth/session.ts b/src/auth/session.ts
        --- a/src/auth/session.ts
        +++ b/src/auth/session.ts
        @@ -10,3 +10,4 @@ export function schedule(session: Session) {
        -  const at = session.expiresAt;
        +  const margin = 60_000;
        +  const at = session.expiresAt - margin;
           timer = setTimeout(refresh, at - Date.now());

        """

    /// A 1×1 PNG.
    private static let pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="
}
