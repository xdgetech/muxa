package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.LoopCheck
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.protocol.WorkspaceInfo

/**
 * A developer's evening: two repos, four agents, one of them asking a question.
 * For Try the Demo and the store screenshots; the text matches the iOS showcase byte for byte.
 */
internal object ShowcaseContent : DemoContent {
    private const val COLS = 120
    private const val ROWS = 36

    override val root = "billing"
    override val chatStatus = AgentState.WAITING
    override val alertDelayMillis = 10_000L

    private fun agent(id: String, window: String, dir: String, state: AgentState, since: Long) =
        Pane(id, window, 0, Role.AGENT, COLS, ROWS, "claude", dir, dead = false, shellForeground = false, agent = Agent(state, since))

    private fun shell(id: String, window: String, dir: String) =
        Pane(id, window, 1, Role.SHELL, COLS, ROWS, "zsh", dir, dead = false, shellForeground = true)

    override fun sessions(now: Long) = listOf(
        Session(
            id = "$1", name = "stripe-webhooks", repo = "billing", repoDir = "~/src/billing",
            branch = "feat/stripe-webhooks", worktree = "~/src/.worktrees/billing-stripe-webhooks", taskId = "t-a1b2c3",
            panes = listOf(
                agent("%1", "@1", "~/src/.worktrees/billing-stripe-webhooks", AgentState.WAITING, now - 75),
                shell("%2", "@1", "~/src/.worktrees/billing-stripe-webhooks"),
            ),
        ),
        Session(
            id = "$2", name = "retry-backoff", repo = "billing", repoDir = "~/src/billing",
            branch = "fix/retry-backoff", worktree = "~/src/.worktrees/billing-retry-backoff", taskId = "t-d4e5f6",
            panes = listOf(
                agent("%3", "@2", "~/src/.worktrees/billing-retry-backoff", AgentState.RUNNING, now - 40),
                shell("%4", "@2", "~/src/.worktrees/billing-retry-backoff"),
            ),
        ),
        Session(
            id = "$3", name = "dark-mode", repo = "dashboard", repoDir = "~/src/dashboard",
            branch = "feat/dark-mode", worktree = "~/src/.worktrees/dashboard-dark-mode", taskId = "t-0a1b2c",
            panes = listOf(agent("%5", "@3", "~/src/.worktrees/dashboard-dark-mode", AgentState.DONE, now - 720)),
        ),
        Session(
            id = "$4", name = "search-index", repo = "dashboard", repoDir = "~/src/dashboard",
            branch = "perf/search-index", worktree = "~/src/.worktrees/dashboard-search-index", taskId = "t-3c4d5e",
            panes = listOf(agent("%6", "@4", "~/src/.worktrees/dashboard-search-index", AgentState.IDLE, now - 1800)),
        ),
        Session(
            id = "$5", name = "notes",
            panes = listOf(
                Pane("%7", "@5", 0, Role.UNKNOWN, COLS, ROWS, "nvim", "~/notes", dead = false, shellForeground = false),
            ),
        ),
    )

    override val conversation = listOf(
        ChatEntry("u:1", ChatEntry.Kind.USER, "Handle Stripe's invoice.paid webhook. Mark the invoice paid and email a receipt."),
        ChatEntry("a:1", ChatEntry.Kind.ASSISTANT, "I'll look at how the other webhooks are routed first."),
        ChatEntry(
            "t:1", ChatEntry.Kind.TOOL,
            tool = ChatTool("Grep", "\"event.type\" in src/webhooks", result = "src/webhooks/router.rs:42"),
        ),
        ChatEntry("t:2", ChatEntry.Kind.TOOL, tool = ChatTool("Read", "src/webhooks/router.rs", result = "118 lines")),
        ChatEntry(
            "a:2", ChatEntry.Kind.ASSISTANT,
            """
            Events are matched in `route()`. I added a handler for `invoice.paid` that verifies the signature, marks the invoice paid in one transaction, and queues the receipt:

            ```rust
            "invoice.paid" => {
                let invoice = Invoice::paid(&tx, &event.data.id).await?;
                jobs::enqueue(&tx, Job::Receipt(invoice.id)).await?;
            }
            ```
            """.trimIndent(),
        ),
        ChatEntry(
            "t:3", ChatEntry.Kind.TOOL,
            tool = ChatTool("Bash", "cargo test -p billing webhooks", result = "test result: ok. 14 passed; 0 failed"),
        ),
        ChatEntry(
            "a:3", ChatEntry.Kind.ASSISTANT,
            """
            All 14 webhook tests pass. Stripe can send the same event twice. Should I store the event id to skip duplicates?

            1. Yes, add an `events` table
            2. No, the update is idempotent already
            """.trimIndent(),
        ),
    )

    override val planUpdate: String? = null
    override val diffUpdate: String? = null

    override val docs = listOf(
        DemoDoc(
            DocInfo("plans/stripe-webhooks.md", DocInfo.Kind.PLAN, "Stripe webhooks", 1),
            """
            # Stripe webhooks

            Pay invoices from Stripe events instead of the nightly sync.

            ```mermaid
            sequenceDiagram
              participant S as Stripe
              participant W as Webhook
              participant DB as Postgres
              participant Q as Jobs
              S->>W: invoice.paid
              W->>W: verify signature
              W->>DB: mark invoice paid
              W->>Q: send receipt
              W-->>S: 200 OK
            ```

            ## Steps

            1. Verify the `Stripe-Signature` header.
            2. Route `invoice.paid` and `invoice.payment_failed`.
            3. Update the invoice and queue the email in one transaction.
            4. Skip events that were already handled.

            ## Risks

            | Risk | Mitigation |
            |---|---|
            | Duplicate events | Store the event id |
            | Slow email provider | Send from the job queue |
            | Signature clock skew | 5 minute tolerance |
            """.trimIndent(),
        ),
        DemoDoc(
            DocInfo("README.md", DocInfo.Kind.REPO, mtime = 1),
            """
            # billing

            Invoices, payments and receipts.

            | Command | Use |
            |---|---|
            | `cargo test` | every test |
            | `make db` | a local Postgres |
            """.trimIndent(),
        ),
        DemoDoc(
            DocInfo("docs/webhooks.md", DocInfo.Kind.REPO, mtime = 1),
            """
            # Webhooks

            Each provider has a route under `/hooks`. A handler verifies the request, then changes data in one transaction.
            """.trimIndent(),
        ),
    )

    override val changes = listOf(
        DemoChange(
            ChangeInfo("src/webhooks/router.rs", ChangeInfo.Status.MODIFIED, added = 9, removed = 2),
            """
            diff --git a/src/webhooks/router.rs b/src/webhooks/router.rs
            index 5c1e2a0..b7d94f3 100644
            --- a/src/webhooks/router.rs
            +++ b/src/webhooks/router.rs
            @@ -38,12 +38,19 @@ pub async fn route(db: &Db, event: Event) -> Result<(), WebhookError> {
                 let mut tx = db.begin().await?;
                 match event.kind.as_str() {
                     "customer.created" => customers::upsert(&mut tx, &event.data).await?,
            -        "invoice.paid" => return Err(WebhookError::Unhandled(event.kind)),
            -        "invoice.payment_failed" => return Err(WebhookError::Unhandled(event.kind)),
            +        "invoice.paid" => {
            +            let invoice = Invoice::paid(&mut tx, &event.data.id).await?;
            +            jobs::enqueue(&mut tx, Job::Receipt(invoice.id)).await?;
            +        }
            +        "invoice.payment_failed" => {
            +            let invoice = Invoice::failed(&mut tx, &event.data.id).await?;
            +            jobs::enqueue(&mut tx, Job::PaymentFailed(invoice.id)).await?;
            +        }
                     other => return Err(WebhookError::Unhandled(other.to_owned())),
                 }
                 tx.commit().await?;
                 Ok(())
             }
            """.trimIndent() + "\n",
        ),
        DemoChange(
            ChangeInfo("src/invoice.rs", ChangeInfo.Status.MODIFIED, added = 14, removed = 0),
            """
            diff --git a/src/invoice.rs b/src/invoice.rs
            index 81aa0c2..e40f1d9 100644
            --- a/src/invoice.rs
            +++ b/src/invoice.rs
            @@ -61,3 +61,17 @@ impl Invoice {
                     .await
                 }
             +
            +    pub async fn paid(tx: &mut Tx, stripe_id: &str) -> Result<Invoice, sqlx::Error> {
            +        sqlx::query_as("UPDATE invoices SET status = 'paid', paid_at = now() WHERE stripe_id = $1 RETURNING *")
            +            .bind(stripe_id)
            +            .fetch_one(&mut **tx)
            +            .await
            +    }
            """.trimIndent() + "\n",
        ),
        DemoChange(
            ChangeInfo("tests/webhooks.rs", ChangeInfo.Status.UNTRACKED, added = 48, removed = 0),
            """
            diff --git a/tests/webhooks.rs b/tests/webhooks.rs
            new file mode 100644
            --- /dev/null
            +++ b/tests/webhooks.rs
            @@ -0,0 +1,6 @@
            +#[tokio::test]
            +async fn invoice_paid_marks_the_invoice_and_queues_a_receipt() {
            +    let app = TestApp::new().await;
            +    app.post_event("invoice_paid.json").await.assert_ok();
            +    assert_eq!(app.invoice("in_1").await.status, Status::Paid);
            +}
            """.trimIndent() + "\n",
        ),
    )

    // The iOS showcase keeps the test fixtures for the branch's commits and the images.
    override val branch = TestContent.branch
    override fun images(now: Long) = TestContent.images(now)

    private const val RESET = "\u001b[0m"
    private const val DIM = "\u001b[2m"
    private const val BOLD = "\u001b[1m"
    private const val GREEN = "\u001b[32m"
    private const val YELLOW = "\u001b[33m"
    private const val BLUE = "\u001b[34m"
    private const val MAGENTA = "\u001b[35m"
    private const val CYAN = "\u001b[36m"
    private const val REMOVED_LINE = "\u001b[48;5;52m"
    private const val ADDED_LINE = "\u001b[48;5;22m"

    override val workspaces = listOf(
        WorkspaceInfo("work", WorkspaceChecks.Only(listOf("make test", "make lint"))),
        WorkspaceInfo("personal"),
    )

    override fun work(now: Long) = DemoWork.showcase(now)

    override fun items(now: Long) = DemoItems.showcase(now)

    override fun assistant(now: Long) = DemoAssistant.showcase(now)

    override fun plans(now: Long) = DemoPlans.showcase(now)

    override fun policies(now: Long) = DemoPolicies.showcase(now)

    override fun tasks(now: Long) = listOf(
        demoTask(
            "t-a1b2c3", "work", "Handle Stripe invoice.paid webhook",
            "Mark the invoice paid when Stripe sends invoice.paid, and email a receipt.",
            TaskStatus.OPEN, now - 75, project = "Billing", repo = "~/src/billing", branch = "feat/stripe-webhooks",
            dir = "~/src/.worktrees/billing-stripe-webhooks", session = "stripe-webhooks",
            runs = listOf(
                demoRun(1, now - 10_875, endedAt = now - 5_475, agentSession = "d3d94468-02a4-4e5c-9b7a-1c2e3f4a5b6c"),
                demoRun(2, now - 5_475, RunReason.RESTART, agentSession = "6512bd43-d9ca-4a6f-8e2b-7d1c0f9e3a45"),
            ),
            loop = demoLoop(
                "make test", LoopState.PAUSED, attempts = 1, since = now - 75, reason = LoopReason.WAITING,
                lastCheck = LoopCheck(1, exit = 2, seconds = 41, at = now - 600),
            ),
        ),
        demoTask(
            "t-d4e5f6", "work", "Retry failed charges with backoff", "Retry a failed charge three times, with exponential backoff and jitter.",
            TaskStatus.OPEN, now - 40, project = "Billing", repo = "~/src/billing", branch = "fix/retry-backoff",
            dir = "~/src/.worktrees/billing-retry-backoff", session = "retry-backoff",
            runs = listOf(demoRun(1, now - 40 - 3_600)),
        ),
        demoTask(
            "t-0a1b2c", "work", "Dark mode for the dashboard", "Add a dark theme that follows the system setting.",
            TaskStatus.OPEN, now - 720, project = "Dashboard", repo = "~/src/dashboard", branch = "feat/dark-mode",
            dir = "~/src/.worktrees/dashboard-dark-mode", session = "dark-mode",
            runs = listOf(demoRun(1, now - 720 - 3_600)),
            loop = demoLoop("make test", LoopState.CHECKING, attempts = 0, since = now - 20),
        ),
        demoTask(
            "t-3c4d5e", "work", "Faster search index", "Cut the search index build time in half.",
            TaskStatus.REVIEW, now - 1_800, project = "Dashboard", repo = "~/src/dashboard", branch = "perf/search-index",
            dir = "~/src/.worktrees/dashboard-search-index", session = "search-index",
            runs = listOf(demoRun(1, now - 1_800 - 3_600)),
            loop = demoLoop(
                "make test", LoopState.PASSED, attempts = 1, since = now - 1_800,
                lastCheck = LoopCheck(2, exit = 0, seconds = 63, at = now - 1_800),
            ),
        ),
        demoTask(
            "t-6f7a8b", "work", "Plan the Q3 pricing change", "Compare our plans with three competitors and propose new prices.",
            TaskStatus.OPEN, now - 5_400, project = "Pricing", dir = "~/work/tasks/plan-the-q3-pricing-change-t-6f7a8b",
        ),
        demoTask(
            "t-9b8c7d", "personal", "File the VAT return", "Collect the quarter's invoices and file the VAT return.",
            TaskStatus.DONE, now - 21_600, dir = "~/personal/tasks/file-the-vat-return-t-9b8c7d",
        ),
    )

    override fun screen(p: Pane): ByteArray {
        val lines = when (p.role) {
            Role.AGENT -> agentScreen
            Role.SHELL -> shellScreen
            else -> editorScreen
        }
        // Rows end with CRLF: the terminal does not add a carriage return on its own.
        return ("\u001bc" + lines.joinToString("\r\n")).toByteArray()
    }

    private val box = "─".repeat(COLS - 2)

    private val agentScreen = listOf(
        "$BOLD> ${RESET}Handle Stripe's invoice.paid webhook. Mark the invoice paid and email a receipt.",
        "",
        "$GREEN●$RESET I'll look at how the other webhooks are routed first.",
        "",
        "$GREEN●$RESET ${BOLD}Grep$RESET(\"event.type\" in src/webhooks)",
        "  $DIM⎿  src/webhooks/router.rs:42$RESET",
        "",
        "$GREEN●$RESET ${BOLD}Read$RESET(src/webhooks/router.rs)",
        "  $DIM⎿  Read 118 lines$RESET",
        "",
        "$GREEN●$RESET ${BOLD}Update$RESET(src/webhooks/router.rs)",
        "  $DIM⎿  Updated src/webhooks/router.rs with 9 additions and 2 removals$RESET",
        "      ${DIM}40$RESET          \"customer.created\" => customers::upsert(&mut tx, &event.data).await?,",
        "      ${DIM}41$RESET $REMOVED_LINE-        \"invoice.paid\" => return Err(WebhookError::Unhandled(event.kind)),$RESET",
        "      ${DIM}41$RESET $ADDED_LINE+        \"invoice.paid\" => {$RESET",
        "      ${DIM}42$RESET $ADDED_LINE+            let invoice = Invoice::paid(&mut tx, &event.data.id).await?;$RESET",
        "      ${DIM}43$RESET $ADDED_LINE+            jobs::enqueue(&mut tx, Job::Receipt(invoice.id)).await?;$RESET",
        "      ${DIM}44$RESET $ADDED_LINE+        }$RESET",
        "",
        "$GREEN●$RESET ${BOLD}Bash$RESET(cargo test -p billing webhooks)",
        "  $DIM⎿  test result: $RESET${GREEN}ok$RESET$DIM. 14 passed; 0 failed; finished in 2.31s$RESET",
        "",
        "$GREEN●$RESET All 14 webhook tests pass. Stripe can send the same event twice.",
        "  Should I store the event id to skip duplicates?",
        "",
        "  $CYAN❯ 1. Yes, add an events table$RESET",
        "    2. No, the update is idempotent already",
        "",
        "$DIM╭$box╮$RESET",
        "$DIM│$RESET > " + " ".repeat(COLS - 5) + "$DIM│$RESET",
        "$DIM╰$box╯$RESET",
        "  $DIM? for shortcuts$RESET",
    )

    private val prompt = "${BLUE}billing$RESET on $MAGENTA feat/stripe-webhooks$RESET $YELLOW!$RESET"

    private val shellScreen = listOf(
        prompt,
        "$GREEN❯$RESET git status --short",
        " ${YELLOW}M$RESET src/invoice.rs",
        " ${YELLOW}M$RESET src/webhooks/router.rs",
        "$DIM??$RESET tests/webhooks.rs",
        "",
        prompt,
        "$GREEN❯$RESET cargo test -p billing",
        "   $GREEN${BOLD}Compiling$RESET billing v0.4.0 (~/src/.worktrees/billing-stripe-webhooks)",
        "    $GREEN${BOLD}Finished$RESET `test` profile [unoptimized + debuginfo] target(s) in 6.42s",
        "     $GREEN${BOLD}Running$RESET tests/webhooks.rs",
        "",
        "running 14 tests",
        "test customer_created_upserts_the_customer ... ${GREEN}ok$RESET",
        "test duplicate_event_is_accepted ... ${GREEN}ok$RESET",
        "test invoice_paid_marks_the_invoice_and_queues_a_receipt ... ${GREEN}ok$RESET",
        "test invoice_payment_failed_queues_a_notice ... ${GREEN}ok$RESET",
        "test signature_with_clock_skew_is_accepted ... ${GREEN}ok$RESET",
        "test unknown_event_is_rejected ... ${GREEN}ok$RESET",
        "test wrong_signature_is_rejected ... ${GREEN}ok$RESET",
        "$DIM…$RESET",
        "",
        "test result: ${GREEN}ok$RESET. 14 passed; 0 failed; 0 ignored; finished in 2.31s",
        "",
        prompt,
        "$GREEN❯$RESET ",
    )

    private val editorScreen = listOf(
        "$BOLD$MAGENTA# Week 43$RESET",
        "",
        "- [x] Stripe webhooks: plan reviewed",
        "- [ ] Retry backoff: check the jitter",
        "- [ ] Dark mode: review screenshots",
        "",
        "$DIM~$RESET",
        "$DIM~$RESET",
    )
}
