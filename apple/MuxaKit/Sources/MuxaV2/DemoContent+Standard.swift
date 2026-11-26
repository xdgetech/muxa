import Foundation

extension DemoContent {
    /// Two Tasks (one waits on the user, one runs Codex, which takes the mouse,
    /// and has a shell beside its agent), a
    /// shell, a terminal muxa did not start, a Routine run, every Inbox kind,
    /// four Routines, and the Bot `pr-watch` with two items (ADR 0082) and a
    /// settings proposal (`d12`, ADR 0083) from its talk `t7`, and a run of
    /// its Routine `scan` (`t8`, ADR 0086). The Bot `scout` has no Routines
    /// and a live talk (`t9`). The Capture assistant suggests
    /// where `d13` goes (ADR 0084). Approving `d8` needs force: its worktree
    /// holds work. `d9` came from muxa v1.
    public static let standard: DemoContent = {
        func agent(_ id: String, _ branch: String, _ state: String, app: String = "claude-code") -> Terminal {
            Terminal(
                id: id, name: branch, live: true, status: TerminalStatus(state: state, app: app),
                labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.branch": branch, "muxa.workspace": "app",
                         "muxa.repo": "/demo/app", "muxa.dir": "/demo/app-\(branch)"],
                cols: 80, rows: 24)
        }
        func item(_ id: String, _ kind: ItemKind, _ title: String, note: String? = nil, workspace: String? = "app",
                  due: String? = nil, source: String, terminal: String? = nil, routine: RoutineRef? = nil,
                  place: ItemPlace? = nil) -> InboxItem {
            InboxItem(id: id, kind: kind, title: title, note: note, workspace: workspace, due: due,
                      created: 1_764_000_000, source: source, terminal: terminal, routine: routine,
                      place: place)
        }
        func bot(_ id: String, _ kind: ItemKind, _ title: String, note: String? = nil, due: String? = nil,
                 links: [ItemLink], proposal: ItemProposal? = nil) -> InboxItem {
            var i = item(id, kind, title, note: note, due: due, source: "routine",
                         routine: RoutineRef(workspace: "app", name: "scan"))
            i.links = links
            i.proposal = proposal
            i.bot = "pr-watch"
            return i
        }
        func routine(_ name: String, at: String?, running: Int64? = nil, last: RoutineLast?) -> Routine {
            Routine(workspace: "app", name: name, enabled: true, at: at, timezone: at.map { _ in "Europe/Berlin" },
                    schedule: nil, nextDue: nil, runningSince: running, last: last)
        }
        let everyRoutine = ["dependency-digest", "nightly-backup", "release-notes"]
        return DemoContent(
            terminals: [
                agent("t1", "fix-login", "working", app: "codex"),
                agent("t2", "docs-audit", "blocked"),
                Terminal(id: "t3", name: "shell", live: true, status: nil, labels: [:], cols: 80, rows: 24),
                // A terminal muxa did not start: no name, no labels.
                Terminal(id: "t4", name: nil, live: true, status: nil, labels: [:], cols: 80, rows: 24),
                Terminal(
                    id: "t5", name: nil, live: true, status: TerminalStatus(state: "working", app: "claude-code"),
                    labels: ["muxa.kind": "routine", "muxa.role": "agent", "muxa.workspace": "app",
                             "muxa.routine": "dependency-digest", "muxa.dir": "/demo/runs/dependency-digest"],
                    cols: 80, rows: 24),
                // fix-login's shell (ADR 0079).
                Terminal(
                    id: "t6", name: "fix-login-sh", live: true, status: nil,
                    labels: ["muxa.kind": "task", "muxa.role": "shell", "muxa.workspace": "app",
                             "muxa.dir": "/demo/app-fix-login"],
                    cols: 80, rows: 24),
                // A talk with pr-watch (ADR 0083); d12 is its proposal.
                Terminal(
                    id: "t7", name: "Talk to pr-watch", live: true, status: TerminalStatus(state: "done", app: "claude-code"),
                    labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.workspace": "app", "muxa.bot": "pr-watch",
                             "muxa.title": "Talk to pr-watch", "muxa.dir": "/demo/tasks/talk-to-pr-watch",
                             "muxa.places": #"[{"kind":"folder","path":"/demo/tasks/talk-to-pr-watch"}]"#],
                    cols: 80, rows: 24),
                // A run of pr-watch's Routine scan (ADR 0086).
                Terminal(
                    id: "t8", name: nil, live: true, status: TerminalStatus(state: "working", app: "claude-code"),
                    labels: ["muxa.kind": "routine", "muxa.role": "agent", "muxa.workspace": "app", "muxa.bot": "pr-watch",
                             "muxa.routine": "scan", "muxa.dir": "/demo/runs/scan"],
                    cols: 80, rows: 24),
                // scout's talk, still working: its page opens on it.
                Terminal(
                    id: "t9", name: "Talk to scout", live: true, status: TerminalStatus(state: "working", app: "claude-code"),
                    labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.workspace": "app", "muxa.bot": "scout",
                             "muxa.title": "Talk to scout", "muxa.dir": "/demo/tasks/talk-to-scout",
                             "muxa.places": #"[{"kind":"folder","path":"/demo/tasks/talk-to-scout"}]"#],
                    cols: 80, rows: 24),
            ],
            screens: [
                // Codex's screen: the alternate screen, and the mouse reported to the program.
                "t1": "\u{1b}[?1049h\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1003h\u{1b}[?1006h"
                    + "• Reading src/auth/session.ts\r\n• Editing src/auth/session.ts\r\n"
                    + "  The session now refreshes before it expires, as https://example.com/auth/refresh describes.\r\n",
                "t2": "● The README links three pages that moved: https://example.com/docs/moved\r\n\r\n"
                    + "  Allow edits to README.md? (y/n) ",
                "t3": "$ ",
                "t4": "% ",
                "t5": "● Reading the lockfile\r\n",
                "t6": "~/demo/app-fix-login $ ",
                "t7": "› Every night, list the PRs nobody touched for a week.\r\n\r\n"
                    + "● I wrote a proposal: a nightly-sweep Routine, turned off. Apply it from the bar above or the Inbox.\r\n",
                "t8": "● Listing the PRs that wait for review\r\n",
                "t9": "› Which of our dependencies have open CVEs?\r\n\r\n● Checking the lockfile against the advisories…\r\n",
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
                bot("d10", .decide, "Approve the docs PR #57?", note: "Two reviewers approved; CI is green.",
                    links: [ItemLink(url: "https://github.com/example/app/pull/57", title: "PR #57: Fix README links")],
                    proposal: ItemProposal(action: "Approve and merge", target: "example/app#57",
                                           content: "Looks good to me. Merging with squash.")),
                bot("d11", .reply, "Anna asks when the release ships", due: "2026-11-25",
                    links: [ItemLink(url: "https://example.slack.com/archives/C01/p1764000000", title: "#release thread")]),
                proposal,
                suggested,
            ],
            routines: [
                // t5 runs it.
                routine("dependency-digest", at: "09:00", running: 1_764_000_000, last: RoutineLast(at: 1_764_000_000, ok: true, reason: nil, published: 1)),
                routine("nightly-backup", at: "02:00", last: RoutineLast(at: 1_764_000_000, ok: false, reason: "gh login expired.", published: nil)),
                routine("release-notes", at: nil, last: nil),
                // t8 runs it.
                Routine(workspace: "app", name: "scan", enabled: true, at: "09:00", timezone: "Europe/Berlin",
                        schedule: "*/30 9-18 * * 1-5", nextDue: 1_764_001_800, runningSince: 1_764_000_000,
                        last: RoutineLast(at: 1_764_000_000, ok: true, reason: nil, published: 2), bot: "pr-watch"),
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
                    DemoFile(entry: ReaderEntry(path: "output/qa/report.md", ignored: true),
                             content: ReaderContent(path: "output/qa/report.md", text: qaReport)),
                ],
                .changes: [
                    DemoFile(entry: ReaderEntry(path: "src/auth/session.ts", status: "modified", added: 2, removed: 1),
                             content: ReaderContent(path: "src/auth/session.ts", text: diff)),
                ],
                .images: [
                    DemoFile(entry: ReaderEntry(path: "screenshots/login.png", size: 70),
                             content: ReaderContent(path: "screenshots/login.png", data: pixel)),
                    DemoFile(entry: ReaderEntry(path: "screenshots/session.png", size: 70),
                             content: ReaderContent(path: "screenshots/session.png", data: pixel)),
                ],
            ],
            chats: [
                "t7": ChatUpdate(reset: true, entries: [
                    ChatEntry(id: "u1", kind: "user", text: "Every night, list the PRs nobody touched for a week."),
                    ChatEntry(id: "a1", kind: "assistant",
                              text: "I proposed a Routine **stale-prs** that runs at 21:00. Apply it to turn it on."),
                ]),
                "t9": ChatUpdate(reset: true, entries: [
                    ChatEntry(id: "u1", kind: "user", text: "Which of our dependencies have open CVEs?"),
                    ChatEntry(id: "a1", kind: "assistant", text: "I'm checking the lockfile against the advisories."),
                ]),
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
            settings: settings,
            subscriptions: usage,
            tokenUsage: tokens)
    }()

    /// Today's tokens: the Tasks' folders are the demo agents' `muxa.dir`.
    private static let tokens: TokenUsage = {
        let day = Date().formatted(.iso8601.year().month().day())
        return TokenUsage(day: day, workspaces: [
            WorkspaceTokens(workspace: "app", input: 1_200_000, cached: 9_400_000, output: 84_000),
            WorkspaceTokens(workspace: "Personal", input: 310_000, cached: 1_900_000, output: 22_000),
        ], tasks: [
            TaskTokens(workspace: "app", dir: "/demo/app-fix-login", title: "app-fix-login",
                       input: 640_000, cached: 5_100_000, output: 41_000),
            TaskTokens(workspace: "app", dir: "/demo/runs/dependency-digest", title: "dependency-digest",
                       input: 220_000, cached: 1_300_000, output: 9_000),
        ])
    }()

    /// The limits the demo's agents last reported (ADR 0103): one near its
    /// 5-hour limit, one never used yet.
    private static let usage: [SubscriptionUsage] = {
        let now = Date()
        let hours = { (h: Double) in now.addingTimeInterval(h * 3600) }
        return [
            SubscriptionUsage(name: "claude-personal", agent: "claude-code", plan: "Max 20x", windows: [
                UsageWindow(minutes: 300, usedPercent: 82, resetsAt: hours(3)),
                UsageWindow(minutes: 10_080, usedPercent: 31, resetsAt: hours(60)),
            ], asOf: now.addingTimeInterval(-240)),
            SubscriptionUsage(name: "claude-spare", agent: "claude-code", plan: "Max 5x"),
            SubscriptionUsage(name: "chatgpt", agent: "codex", plan: "Plus", windows: [
                UsageWindow(minutes: 300, usedPercent: 18, resetsAt: hours(1.5)),
                UsageWindow(minutes: 10_080, usedPercent: 41, resetsAt: hours(90)),
            ], asOf: now.addingTimeInterval(-720)),
        ]
    }()

    /// What pr-watch proposed in a talk: a turned-off Routine of its own and
    /// the commands it may run, against the demo's first settings revision.
    private static let proposal: InboxItem = {
        let sweep = RoutineSpec(name: "nightly-sweep", at: "02:30", timezone: "Europe/Berlin",
                                instructions: "routines/nightly-sweep.md", enabled: false, bot: "pr-watch", readOnly: true)
        let watch = BotSpec(name: "pr-watch", description: "Watches PRs and threads that wait for me",
                            commands: ["gh pr list:*", "gh pr diff:*"])
        var i = InboxItem(
            id: "d12", kind: .decide, title: "pr-watch proposes: a nightly sweep of stale PRs",
            note: "- routine-add nightly-sweep (off)\n- Change Bot pr-watch, may run: gh pr list:*; gh pr diff:*\n", workspace: "app", due: nil,
            created: 1_764_000_000, source: "task", terminal: "t7", routine: nil)
        i.bot = "pr-watch"
        i.settingsProposal = SettingsProposal(revision: "1", ops: [
            .routineAdd(workspace: "app", routine: sweep,
                        instructionsText: "List the PRs with no activity for a week, oldest first.\n"),
            .botUpdate(workspace: "app", bot: watch),
        ])
        return i
    }()

    /// A capture the Capture assistant has a suggestion for.
    private static let suggested: InboxItem = {
        var i = InboxItem(
            id: "d13", kind: .todo, title: "Look into the flaky login test", note: nil, workspace: nil, due: nil,
            created: 1_764_000_000, source: "capture", terminal: nil, routine: nil)
        i.suggestion = ItemSuggestion(
            workspace: "app", due: "2026-11-28", note: "It fails about once in ten CI runs.",
            taskGoal: "Find why the login test fails now and then, and fix it.", bot: "pr-watch")
        return i
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
                    SettingsRoutine(
                        spec: RoutineSpec(name: "scan", schedule: "*/30 9-18 * * 1-5", timezone: "Europe/Berlin",
                                          instructions: "routines/scan.md", bot: "pr-watch", readOnly: true),
                        instructionsText: "List the PRs that wait for my review, and the threads that ask me something.\n"),
                ],
                foundRepos: ["app", "docs", "legacy"], hiddenRepos: ["legacy"],
                botRules: BotRules(allow: ["WebSearch", "WebFetch"], deny: ["Bash"]),
                bots: [
                    SettingsBot(
                        spec: BotSpec(name: "pr-watch", description: "Watches PRs and threads that wait for me",
                                      commands: ["gh pr list:*"]),
                        soulText: "I watch the PRs that wait for my review and the threads that ask me something.\n"
                            + "I never comment or merge by myself.\n",
                        memoryText: "Anna owns the release. The docs repo merges with squash.\n",
                        memoryVersions: 2,
                        memoryHistory: [MemoryVersion(version: 1, at: 1_764_000_240),
                                        MemoryVersion(version: 2, at: 1_763_913_600)]),
                    SettingsBot(
                        spec: BotSpec(name: "scout", description: "Looks into the questions I hand it"),
                        soulText: "I answer one question at a time and cite where I found it.\n", memoryText: ""),
                ]),
            SettingsWorkspace(
                name: "Personal", root: "/demo/personal", agent: SettingsAgent(command: "claude"),
                routines: [
                    SettingsRoutine(
                        spec: RoutineSpec(name: "weekly-review", at: "18:00", timezone: "Europe/Berlin",
                                          instructions: "routines/weekly-review.md"),
                        instructionsText: "Sum up the week's Done items.\n"),
                ],
                subscriptions: ["claude-personal", "claude-spare"]),
        ],
        workers: [WorkerSpec(name: "reviewer", description: "Reviews diffs", agent: "claude-code")],
        assistant: SettingsAssistant(
            enabled: true,
            soulText: "I sort what you capture: the Workspace, a due date, and whether a Bot can help.\n",
            memoryText: "Release items go to app. Personal errands have no Workspace.\n"),
        subscriptions: [
            SubscriptionSpec(name: "claude-personal", agent: "claude-code", home: "~/.claude", plan: "Max 20x"),
            SubscriptionSpec(name: "claude-spare", agent: "claude-code", home: "~/.claude-spare", plan: "Max 5x"),
            SubscriptionSpec(name: "chatgpt", agent: "codex", home: "~/.codex", plan: "Plus"),
        ],
        // app's env names its own Claude login: onboarding offers to adopt it.
        foundLogins: [FoundLogin(agent: "claude-code", workspace: "app")],
        build: SettingsBuild(dir: "/Volumes/Build/muxa", dirFound: true, extraPaths: ["out"]))

    private static let plan = """
        # Plan

        1. Refresh the session 60 s before it expires.
        2. Retry once when the refresh fails.
        3. Add a test for a session that expired while the app slept.

        The goal is in [the Task's brief](.muxa/TASK.md).

        """

    private static let qaReport = """
        # QA report

        - [x] A session refreshes 60 s before it expires.
        - [x] A failed refresh retries once.

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
