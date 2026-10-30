package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ImageInfo
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session

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
}

/** The fixtures the UI tests assert on. */
internal object TestContent : DemoContent {
    override val root = "app"
    override val chatStatus = AgentState.RUNNING
    override val alertDelayMillis = 4_000L

    /** Shells with nothing run yet: a title bar on the first row and the prompt under it. */
    val freshShells = setOf("%8")

    override fun sessions(now: Long) = listOf(
        Session(
            id = "$1", name = "feature-auth", repo = "app", repoDir = "/src/app", branch = "feat/auth",
            worktree = "/src/.worktrees/app-feat-auth",
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
            worktree = "/src/.worktrees/app-fix-login",
            panes = listOf(
                Pane(
                    "%5", "@4", 0, Role.AGENT, 100, 30, "claude", "/work", dead = false, shellForeground = false,
                    agent = Agent(AgentState.IDLE, now - 600),
                ),
            ),
        ),
        Session(
            id = "$5", name = "docs-site", repo = "docs", repoDir = "/src/docs", branch = "main",
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
