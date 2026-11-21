import Foundation

extension DemoContent {
    /// Two Tasks (one waits on the user), a shell, every Inbox kind, and three Routines.
    public static let standard: DemoContent = {
        func agent(_ id: String, _ branch: String, _ state: String) -> Terminal {
            Terminal(
                id: id, name: branch, live: true, status: TerminalStatus(state: state, app: "claude-code"),
                labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.branch": branch,
                         "muxa.repo": "/demo/app", "muxa.dir": "/demo/app-\(branch)"],
                cols: 80, rows: 24)
        }
        func item(_ id: String, _ kind: ItemKind, _ title: String, note: String? = nil, workspace: String? = "app",
                  due: String? = nil, source: String) -> InboxItem {
            InboxItem(id: id, kind: kind, title: title, note: note, workspace: workspace, due: due,
                      laterUntil: nil, created: 1_764_000_000, source: source)
        }
        func routine(_ name: String, at: String?, last: RoutineLast?) -> Routine {
            Routine(workspace: "app", name: name, enabled: true, at: at, timezone: at.map { _ in "Europe/Berlin" },
                    schedule: nil, nextDue: nil, runningSince: nil, last: last)
        }
        return DemoContent(
            terminals: [
                agent("t1", "fix-login", "working"),
                agent("t2", "docs-audit", "blocked"),
                Terminal(id: "t3", name: "shell", live: true, status: nil, labels: [:], cols: 80, rows: 24),
            ],
            screens: [
                "t1": "● Reading src/auth/session.ts\r\n● Editing src/auth/session.ts\r\n  The session now refreshes before it expires.\r\n",
                "t2": "● The README links three pages that moved.\r\n\r\n  Allow edits to README.md? (y/n) ",
                "t3": "$ ",
            ],
            inbox: [
                item("d1", .review, "Review fix-login", note: "Draft PR #42: refresh the session before it expires.", source: "task"),
                item("d2", .reply, "Reply to Anna about the launch date", workspace: "Personal", due: "2026-11-27", source: "capture"),
                item("d3", .decide, "Upgrade the database to Postgres 17?", note: "Two extensions need a new version first.", source: "routine"),
                item("d4", .read, "Weekly dependency digest", note: "3 updates, 1 security fix.", source: "routine"),
                item("d5", .failed, "nightly-backup failed", note: "gh login expired.", source: "routine"),
                item("d6", .todo, "Renew the domain", workspace: "Personal", due: "2026-12-01", source: "capture"),
            ],
            routines: [
                routine("dependency-digest", at: "09:00", last: RoutineLast(at: 1_764_000_000, ok: true, reason: nil, published: 1)),
                routine("nightly-backup", at: "02:00", last: RoutineLast(at: 1_764_000_000, ok: false, reason: "gh login expired.", published: nil)),
                routine("release-notes", at: nil, last: nil),
            ],
            workspaces: ["app", "Personal"])
    }()
}
