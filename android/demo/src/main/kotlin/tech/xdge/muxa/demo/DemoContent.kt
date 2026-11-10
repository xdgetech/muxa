package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentRun
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ImageInfo
import tech.xdge.muxa.protocol.LoopCheck
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.protocol.WorkspaceInfo

internal class DemoDoc(val info: DocInfo, val text: String)

/** [diff] is null for a binary file. */
internal class DemoChange(val info: ChangeInfo, val diff: String?)

/** [color] is 0xRRGGBB. */
internal class DemoImage(val info: ImageInfo, val color: Int)

/** What the demo shows: the sessions, the chat on %1, and the Reader's repo. */
internal interface DemoContent {
    /** The repo the Reader shows. */
    val root: String
    val chatStatus: AgentState

    /** With `alerts`: when another agent starts waiting, after connecting. */
    val alertDelayMillis: Long
    fun sessions(now: Long): List<Session>

    /** Panes whose window follows the device in use, as on a host with `window-size latest` (ADR 0020). */
    val followers: Set<String> get() = emptySet()
    val conversation: List<ChatEntry>
    val docs: List<DemoDoc>

    /** Appended while a plan or a diff is open, to show that the Reader follows the file. */
    val planUpdate: String?
    val diffUpdate: String?
    val changes: List<DemoChange>

    /** What the task branch's commits changed since it left main. */
    val branch: List<DemoChange>
    fun images(now: Long): List<DemoImage>
    fun screen(p: Pane): ByteArray

    /** The host's workspaces, and its task records: one in each Inbox group. The iOS demo serves the same records. */
    val workspaces: List<WorkspaceInfo>
    fun tasks(now: Long): List<TaskRecord>

    /** The host's Routines and stored Inbox entries (ADR 0026). */
    fun work(now: Long): DemoWork

    /** The Tasks' Inbox items (ADR 0033); a host at `work` level 3 serves them. */
    fun items(now: Long): DemoItems = DemoItems(emptyList())

    /** Captures and Work items (ADR 0048); a host at `work` level 9 serves them. */
    fun assistant(now: Long): DemoAssistant = DemoAssistant(emptyList(), emptyList())

    /** The overall assistant and its plans (ADR 0049); a host at `work` level 10 serves them. */
    fun plans(now: Long): DemoPlans? = null

    /** Tasks whose branch is not merged: ending one with `worktree: remove` is `would-lose-work` (ADR 0043). */
    val unmerged: Set<String> get() = emptySet()
}

/** A record as the bridge writes one; [rev] 1 unless it was changed since. */
internal fun demoTask(
    id: String, workspace: String, title: String, goal: String, status: TaskStatus, updatedAt: Long,
    project: String? = null, repo: String? = null, branch: String? = null, dir: String? = null,
    session: String? = null, note: String? = null, runs: List<AgentRun> = emptyList(), loop: TaskLoop? = null,
) = TaskRecord(
    id, rev = 1, workspace = workspace, title = title, goal = goal, status = status,
    createdAt = updatedAt - 3_600, updatedAt = updatedAt, project = project, repo = repo, branch = branch,
    dir = dir, session = session, note = note, runs = runs, loop = loop,
)

/** A Task Loop with the host's defaults, which changed state at [since]; the iOS demo serves the same ones. */
internal fun demoLoop(
    check: String, state: LoopState, attempts: Int, since: Long, reason: LoopReason? = null, lastCheck: LoopCheck? = null,
    maxAttempts: Int = 3, timeout: Long = 600,
) = TaskLoop(check, maxAttempts, timeout, state, attempts, armedAt = since, since = since, reason = reason, lastCheck = lastCheck)

/** An agent run; the iOS demo serves the same ones. */
internal fun demoRun(
    id: Long, startedAt: Long, reason: RunReason = RunReason.START, agent: String = "claude-code",
    endedAt: Long? = null, agentSession: String? = null, transcript: String? = null,
) = AgentRun(id, agent, startedAt, endedAt, agentSession, transcript, reason)

/** The fixtures the UI tests assert on. */
internal object TestContent : DemoContent {
    override val root = "app"
    override val chatStatus = AgentState.RUNNING
    override val alertDelayMillis = 4_000L

    /** Shells with nothing run yet: a title bar on the first row and the prompt under it. */
    val freshShells = setOf("%8")

    override val followers = setOf("%9")

    override fun sessions(now: Long) = listOf(
        Session(
            id = "$1", name = "feature-auth", repo = "app", repoDir = "/src/app", branch = "feat/auth",
            worktree = "/src/.worktrees/app-feat-auth", taskId = "t-auth01",
            panes = listOf(
                Pane(
                    "%1", "@1", 0, Role.AGENT, 100, 30, "claude", "/work", dead = false, shellForeground = false,
                    agent = Agent(AgentState.WAITING, now - 90),
                ),
                Pane("%2", "@1", 1, Role.SHELL, 100, 30, "zsh", "/work", dead = false, shellForeground = true),
            ),
        ),
        Session(
            id = "$4", name = "fix-login", repo = "app", repoDir = "/src/app", branch = "fix/login",
            worktree = "/src/.worktrees/app-fix-login", taskId = "t-login1",
            panes = listOf(
                Pane(
                    "%5", "@4", 0, Role.AGENT, 100, 30, "claude", "/work", dead = false, shellForeground = false,
                    agent = Agent(AgentState.IDLE, now - 600),
                ),
            ),
        ),
        Session(
            id = "$5", name = "docs-site", repo = "docs", repoDir = "/src/docs", branch = "main", taskId = "t-docs01",
            panes = listOf(
                Pane(
                    "%6", "@5", 0, Role.AGENT, 100, 30, "claude", "/docs", dead = false, shellForeground = false,
                    agent = Agent(AgentState.RUNNING, now - 20),
                ),
            ),
        ),
        Session(
            id = "$2", name = "scratch",
            panes = listOf(Pane("%3", "@2", 0, Role.UNKNOWN, 132, 43, "vim", "/tmp", dead = false, shellForeground = false)),
        ),
        // Taller than a landscape tablet at a readable size: it needs fit width and scrolling.
        Session(
            id = "$3", name = "tall",
            panes = listOf(Pane("%4", "@3", 0, Role.SHELL, 100, 60, "zsh", "/tmp", dead = false, shellForeground = true)),
        ),
        // A tall agent with its prompt at the bottom, beside a shell that has just started.
        Session(
            id = "$6", name = "pair",
            panes = listOf(
                Pane("%7", "@6", 0, Role.AGENT, 100, 60, "claude", "/tmp", dead = false, shellForeground = false),
                Pane("%8", "@6", 1, Role.SHELL, 100, 60, "zsh", "/tmp", dead = false, shellForeground = true),
            ),
        ),
        // Made detached at tmux's default 80x24; its window follows the device (ADR 0020).
        Session(
            id = "$7", name = "device",
            panes = listOf(Pane("%9", "@7", 0, Role.SHELL, 80, 24, "zsh", "/tmp", dead = false, shellForeground = true)),
        ),
    )

    override val conversation = listOf(
        ChatEntry("u:1", ChatEntry.Kind.USER, "Fix the failing test in the parser"),
        ChatEntry("a:1", ChatEntry.Kind.ASSISTANT, "I'll run the tests first to see what fails."),
        ChatEntry(
            "t:1", ChatEntry.Kind.TOOL,
            tool = ChatTool("Bash", "cargo test -p parser", result = "test parses_empty ... FAILED", error = true),
        ),
        ChatEntry(
            "a:2", ChatEntry.Kind.ASSISTANT,
            """
            `parses_empty` expects an empty list, but the parser returns `None`:

            ```rust
            pub fn parse(input: &str) -> Option<Vec<Item>> {
                if input.is_empty() { return None; }
            ```

            I'll return an empty list instead.
            """.trimIndent(),
        ),
        ChatEntry("t:2", ChatEntry.Kind.TOOL, tool = ChatTool("Edit", "src/parser.rs", result = "Updated 1 line.")),
        ChatEntry("n:1", ChatEntry.Kind.NOTICE, "/model opus"),
    )

    override val planUpdate: String? = "\n## Update\n\nUpdated by the agent while you read.\n"

    override val docs = listOf(
        DemoDoc(
            DocInfo("plans/demo-plan.md", DocInfo.Kind.PLAN, "Reader plan", 1),
            """
            # Reader plan

            ## Context

            Plans and designs are read on the iPad. ${"Long context keeps this page scrolling. ".repeat(30)}

            ## Phases

            1. List the documents.
            2. Render them with diagrams.
            3. Follow changes.

            ## Risks

            ${"A long section about risks. ".repeat(40)}

            ## Steps

            ${(1..40).joinToString("\n            ") { "$it. A step of the plan." }}

            The end of the plan.
            """.trimIndent(),
        ),
        DemoDoc(
            DocInfo("README.md", DocInfo.Kind.REPO, mtime = 1),
            """
            # App

            | Command | Use |
            |---|---|
            | `make check` | every check |
            | `make sandbox` | a test server |

            See the [design](docs/design.md), or the [website](https://example.com).
            """.trimIndent(),
        ),
        DemoDoc(
            DocInfo("docs/design.md", DocInfo.Kind.REPO, mtime = 1),
            """
            # Design

            ```mermaid
            flowchart LR
              iPad[iPad app] --> Bridge[muxa bridge] --> Tmux[tmux server]
            ```

            ```swift
            let answer = 42
            ```
            """.trimIndent(),
        ),
    )

    override val diffUpdate: String? = "+    // checked by the agent\n"

    override val changes = listOf(
        DemoChange(
            ChangeInfo("src/parser.rs", ChangeInfo.Status.MODIFIED, added = 2, removed = 1),
            """
            diff --git a/src/parser.rs b/src/parser.rs
            index 3b18e51..a9c2f04 100644
            --- a/src/parser.rs
            +++ b/src/parser.rs
            @@ -10,7 +10,8 @@ use crate::item::Item;
             /// Parses a list of items.
             pub fn parse(input: &str) -> Option<Vec<Item>> {
            -    if input.is_empty() { return None; }
            +    if input.is_empty() {
            +        return Some(Vec::new());
            +    }
                 let items = split(input)?;
                 Some(items)
             }
            """.trimIndent() + "\n",
        ),
        DemoChange(
            ChangeInfo("docs/notes.md", ChangeInfo.Status.UNTRACKED, added = 3, removed = 0),
            """
            diff --git a/docs/notes.md b/docs/notes.md
            new file mode 100644
            --- /dev/null
            +++ b/docs/notes.md
            @@ -0,0 +1,3 @@
            +# Notes
            +
            +Empty input is a valid, empty list.
            """.trimIndent() + "\n",
        ),
        DemoChange(ChangeInfo("assets/logo.png", ChangeInfo.Status.ADDED), null),
    )

    override val branch = listOf(
        DemoChange(
            ChangeInfo("src/item.rs", ChangeInfo.Status.MODIFIED, added = 1, removed = 0),
            """
            diff --git a/src/item.rs b/src/item.rs
            index 1c2d3e4..5f6a7b8 100644
            --- a/src/item.rs
            +++ b/src/item.rs
            @@ -1,3 +1,4 @@
             /// One parsed item.
            +#[derive(Debug, Clone, PartialEq)]
             pub struct Item {
                 pub name: String,
            """.trimIndent() + "\n",
        ),
    )

    /** Screenshots of a QA run, newest first, and the repo's logo. Colors are UIKit's system colors. */
    override fun images(now: Long) = listOf(
        DemoImage(ImageInfo("build/screenshots/en/login.png", now - 60, 48_213), 0x007AFF),
        DemoImage(ImageInfo("build/screenshots/en/home.png", now - 61, 51_002), 0x30B0C7),
        DemoImage(ImageInfo("build/screenshots/ja/login.png", now - 62, 49_870), 0x5856D6),
        DemoImage(ImageInfo("docs/images/logo.png", now - 86_400, 9_120), 0xFF9500),
    )

    /** Checks from a list, any check, and no loops at all (ADR 0019). */
    override val workspaces = listOf(
        // Repos under the root show by their relative path, the one outside it with `~` (ADR 0031).
        WorkspaceInfo(
            "work", WorkspaceChecks.Only(listOf("make check", "cargo test --workspace")),
            listOf("/home/dev/forks/tmux", "/src/app", "/src/docs", "/src/libs/core"), root = "/src",
        ),
        WorkspaceInfo("personal", WorkspaceChecks.Any),
        WorkspaceInfo("notes"),
    )

    /** Every Inbox group, and every loop state: paused, armed, checking, passed, and stopped by limit, timeout and the user. */
    override fun tasks(now: Long) = listOf(
        demoTask(
            "t-auth01", "work", "Add sign-in with passkeys", "Let users sign in with a passkey instead of a password.",
            TaskStatus.OPEN, now - 90, project = "App", repo = "/src/app", branch = "feat/auth",
            dir = "/src/.worktrees/app-feat-auth", session = "feature-auth",
            // The agent waits on a menu, so the loop is paused.
            loop = demoLoop(
                "make check", LoopState.PAUSED, attempts = 1, since = now - 90, reason = LoopReason.WAITING,
                lastCheck = LoopCheck(1, exit = 2, seconds = 48, at = now - 400),
            ),
            runs = listOf(
                demoRun(
                    1, now - 7_290, endedAt = now - 3_690, agentSession = "8f14e45f-ceea-4b6a-9f2b-0c3e5d7a1b21",
                    transcript = "/home/dev/.claude/projects/-src--worktrees-app-feat-auth/8f14e45f-ceea-4b6a-9f2b-0c3e5d7a1b21.jsonl",
                ),
                demoRun(2, now - 3_690, RunReason.RESTART, agentSession = "c9f0f895-fb98-4b9e-8a3d-2f1e6c4b7d90"),
            ),
        ),
        demoTask(
            "t-login1", "work", "Fix the login redirect", "After sign-in, return users to the page they came from.",
            TaskStatus.OPEN, now - 600, project = "App", repo = "/src/app", branch = "fix/login",
            dir = "/src/.worktrees/app-fix-login", session = "fix-login",
            runs = listOf(demoRun(1, now - 4_200, agentSession = "45c48cce-2e2d-4fbd-a1f4-6b3c8e9d0a12")),
            // The agent finished its turn, so the check runs.
            loop = demoLoop("cargo test --workspace", LoopState.CHECKING, attempts = 0, since = now - 15),
        ),
        demoTask(
            "t-docs01", "work", "Update the docs site", "Document the new settings page.",
            TaskStatus.OPEN, now - 20, project = "Docs", repo = "/src/docs", branch = "main", dir = "/src/docs", session = "docs-site",
            runs = listOf(demoRun(1, now - 1_220)),
            loop = demoLoop(
                "make check", LoopState.ARMED, attempts = 2, since = now - 200,
                lastCheck = LoopCheck(2, exit = 1, seconds = 37, at = now - 210),
            ),
        ),
        demoTask(
            "t-price1", "work", "Draft the pricing page", "Write the copy for the new pricing page, with three plans.",
            TaskStatus.OPEN, now - 1_200, project = "Launch", dir = "/work/tasks/draft-the-pricing-page-t-price1",
            note = "The agent did not start: no agent command is set for this workspace.",
        ),
        demoTask(
            "t-relnot", "work", "Write the release notes", "Summarize the changes since 1.2 for the blog.",
            TaskStatus.DONE, now - 10_800, project = "Launch", dir = "/work/tasks/write-the-release-notes-t-relnot",
            runs = listOf(demoRun(1, now - 90_000, agent = "codex", agentSession = "rollout-2026-10-31T09-12-44")),
            loop = demoLoop(
                "make check", LoopState.PASSED, attempts = 1, since = now - 86_500,
                lastCheck = LoopCheck(2, exit = 0, seconds = 52, at = now - 86_500),
            ),
        ),
        demoTask(
            "t-lint01", "personal", "Fix the lint warnings", "Make npm run lint pass without warnings.",
            TaskStatus.OPEN, now - 300, dir = "/personal/tasks/fix-the-lint-warnings-t-lint01",
            loop = demoLoop(
                "npm run lint", LoopState.STOPPED, attempts = 3, since = now - 300, reason = LoopReason.LIMIT,
                lastCheck = LoopCheck(3, exit = 1, seconds = 14, at = now - 300),
            ),
        ),
        demoTask(
            "t-perf01", "work", "Speed up the test suite", "Cut the test suite's run time below five minutes.",
            TaskStatus.OPEN, now - 900, project = "App", repo = "/src/app", branch = "perf/tests",
            dir = "/src/.worktrees/app-perf-tests",
            loop = demoLoop(
                "cargo test --workspace", LoopState.STOPPED, attempts = 1, since = now - 900, reason = LoopReason.TIMEOUT,
                lastCheck = LoopCheck(2, exit = null, seconds = 600, at = now - 900),
            ),
        ),
        demoTask(
            "t-deps01", "work", "Update the dependencies", "Update every dependency to its latest minor version.",
            TaskStatus.OPEN, now - 3_000, project = "App", dir = "/work/tasks/update-the-dependencies-t-deps01",
            loop = demoLoop(
                "make check", LoopState.STOPPED, attempts = 1, since = now - 3_000, reason = LoopReason.USER,
                lastCheck = LoopCheck(1, exit = 2, seconds = 51, at = now - 3_100),
            ),
        ),
        demoTask(
            "t-notes1", "notes", "Sort the meeting notes", "File last week's meeting notes by project.",
            TaskStatus.OPEN, now - 7_200, dir = "/notes/tasks/sort-the-meeting-notes-t-notes1",
        ),
    )

    override fun work(now: Long) = DemoWork.tests(now)

    override fun items(now: Long) = DemoItems.tests(now)

    override fun assistant(now: Long) = DemoAssistant.tests(now)

    override fun plans(now: Long) = DemoPlans.tests(now)

    override val unmerged = setOf("t-perf01")

    /** A frame that shows whether the grid is exact: corners, a ruler and the last row. */
    override fun screen(p: Pane): ByteArray {
        val cols = p.cols
        val rows = p.rows
        val title = "${p.command} — pane ${p.id} ${cols}x$rows"
        val s = StringBuilder("\u001bc")
        if (p.id in freshShells) {
            s.append("\u001b[1;1H\u001b[7m").append(title.padEnd(cols).take(cols))
            s.append("\u001b[0m\u001b[2;1H$ ")
            return s.toString().toByteArray()
        }
        val ruler = (0 until cols).joinToString("") { (it % 10).toString() }
        s.append("\u001b[1;1H\u001b[36m").append(ruler).append("\u001b[0m")
        s.append("\u001b[2;1H\u001b[1m").append(title).append("\u001b[0m")
        s.append("\u001b[4;1H日本語 ✓ é  \u001b[31mred\u001b[32m green\u001b[34m blue\u001b[0m")
        s.append("\u001b[$rows;1H\u001b[7m").append(" ".repeat(cols - 1)).append("\u001b[0m")
        s.append("\u001b[$rows;${cols}HX")
        // A tall pane keeps its prompt near the bottom, like an agent's composer.
        s.append("\u001b[${if (rows > 50) rows - 1 else 6};1H$ ")
        return s.toString().toByteArray()
    }
}
