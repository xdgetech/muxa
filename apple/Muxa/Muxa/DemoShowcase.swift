import Foundation
import MuxaProtocol

/// What the demo bridge serves. `tests` holds the fixtures the UI tests assert on;
/// `showcase` looks like real work, for Try the Demo and the store screenshots.
struct DemoContent: Sendable {
    var sessions: [Session]
    var conversation: [ChatEntry]
    var chatStatus: AgentState
    /// The repo the Reader shows.
    var root: String
    var docs: [(info: DocInfo, text: String)]
    var changes: [(info: ChangeInfo, diff: String?)]
    /// Appended while a plan or a diff is open, to show that the Reader follows the file.
    var planUpdate: String?
    var diffUpdate: String?
    /// With alerts on: when another agent starts waiting, in seconds after connecting.
    var alertDelay: Double
    var screen: @Sendable (Pane) -> Data

    #if DEBUG
    /// The UI tests' launch argument `-showcase` picks the showcase.
    static var current: DemoContent {
        ProcessInfo.processInfo.arguments.contains("-showcase") ? .showcase : .tests
    }
    #endif

    static let tests = DemoContent(
        sessions: DemoBridge.initialSessions, conversation: DemoBridge.conversation, chatStatus: .running,
        root: "app", docs: DemoBridge.docs, changes: DemoBridge.changes,
        planUpdate: DemoBridge.planUpdate, diffUpdate: DemoBridge.diffUpdate, alertDelay: 4, screen: DemoBridge.screen(for:))

    static let showcase = DemoContent(
        sessions: Showcase.sessions, conversation: Showcase.conversation, chatStatus: .waiting,
        root: "billing", docs: Showcase.docs, changes: Showcase.changes,
        planUpdate: nil, diffUpdate: nil, alertDelay: 10, screen: Showcase.screen(for:))
}

/// A developer's evening: two repos, four agents, one of them asking a question.
enum Showcase {
    private static let now = UInt64(Date().timeIntervalSince1970)
    private static let cols: UInt16 = 120
    private static let rows: UInt16 = 36

    private static func agent(_ id: String, _ window: String, _ dir: String, _ state: AgentState, ago: UInt64) -> Pane {
        Pane(id: id, window: window, index: 0, role: .agent, cols: cols, rows: rows, command: "claude",
             path: dir, dead: false, shellForeground: false, agent: Agent(state: state, since: now - ago))
    }

    private static func shell(_ id: String, _ window: String, _ dir: String) -> Pane {
        Pane(id: id, window: window, index: 1, role: .shell, cols: cols, rows: rows, command: "zsh",
             path: dir, dead: false, shellForeground: true)
    }

    static let sessions = [
        Session(id: "$1", name: "stripe-webhooks", repo: "billing", repoDir: "~/src/billing",
                branch: "feat/stripe-webhooks", worktree: "~/src/.worktrees/billing-stripe-webhooks", panes: [
            agent("%1", "@1", "~/src/.worktrees/billing-stripe-webhooks", .waiting, ago: 75),
            shell("%2", "@1", "~/src/.worktrees/billing-stripe-webhooks"),
        ]),
        Session(id: "$2", name: "retry-backoff", repo: "billing", repoDir: "~/src/billing",
                branch: "fix/retry-backoff", worktree: "~/src/.worktrees/billing-retry-backoff", panes: [
            agent("%3", "@2", "~/src/.worktrees/billing-retry-backoff", .running, ago: 40),
            shell("%4", "@2", "~/src/.worktrees/billing-retry-backoff"),
        ]),
        Session(id: "$3", name: "dark-mode", repo: "dashboard", repoDir: "~/src/dashboard",
                branch: "feat/dark-mode", worktree: "~/src/.worktrees/dashboard-dark-mode", panes: [
            agent("%5", "@3", "~/src/.worktrees/dashboard-dark-mode", .done, ago: 720),
        ]),
        Session(id: "$4", name: "search-index", repo: "dashboard", repoDir: "~/src/dashboard",
                branch: "perf/search-index", worktree: "~/src/.worktrees/dashboard-search-index", panes: [
            agent("%6", "@4", "~/src/.worktrees/dashboard-search-index", .idle, ago: 1800),
        ]),
        Session(id: "$5", name: "notes", panes: [
            Pane(id: "%7", window: "@5", index: 0, role: .unknown, cols: cols, rows: rows, command: "nvim",
                 path: "~/notes", dead: false, shellForeground: false),
        ]),
    ]

    static let conversation: [ChatEntry] = [
        ChatEntry(id: "u:1", kind: .user, text: "Handle Stripe's invoice.paid webhook. Mark the invoice paid and email a receipt."),
        ChatEntry(id: "a:1", kind: .assistant, text: "I'll look at how the other webhooks are routed first."),
        ChatEntry(id: "t:1", kind: .tool, tool: ChatTool(
            name: "Grep", summary: "\"event.type\" in src/webhooks", result: "src/webhooks/router.rs:42")),
        ChatEntry(id: "t:2", kind: .tool, tool: ChatTool(
            name: "Read", summary: "src/webhooks/router.rs", result: "118 lines")),
        ChatEntry(id: "a:2", kind: .assistant, text: """
            Events are matched in `route()`. I added a handler for `invoice.paid` that verifies \
            the signature, marks the invoice paid in one transaction, and queues the receipt:

            ```rust
            "invoice.paid" => {
                let invoice = Invoice::paid(&tx, &event.data.id).await?;
                jobs::enqueue(&tx, Job::Receipt(invoice.id)).await?;
            }
            ```
            """),
        ChatEntry(id: "t:3", kind: .tool, tool: ChatTool(
            name: "Bash", summary: "cargo test -p billing webhooks", result: "test result: ok. 14 passed; 0 failed")),
        ChatEntry(id: "a:3", kind: .assistant, text: """
            All 14 webhook tests pass. Stripe can send the same event twice. \
            Should I store the event id to skip duplicates?

            1. Yes, add an `events` table
            2. No, the update is idempotent already
            """),
    ]

    static let docs: [(info: DocInfo, text: String)] = [
        (DocInfo(path: "plans/stripe-webhooks.md", kind: .plan, title: "Stripe webhooks", mtime: 1), """
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
            """),
        (DocInfo(path: "README.md", kind: .repo, mtime: 1), """
            # billing

            Invoices, payments and receipts.

            | Command | Use |
            |---|---|
            | `cargo test` | every test |
            | `make db` | a local Postgres |
            """),
        (DocInfo(path: "docs/webhooks.md", kind: .repo, mtime: 1), """
            # Webhooks

            Each provider has a route under `/hooks`. A handler verifies the request, \
            then changes data in one transaction.
            """),
    ]

    static let changes: [(info: ChangeInfo, diff: String?)] = [
        (ChangeInfo(path: "src/webhooks/router.rs", status: .modified, added: 9, removed: 2), """
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

            """),
        (ChangeInfo(path: "src/invoice.rs", status: .modified, added: 14, removed: 0), """
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

            """),
        (ChangeInfo(path: "tests/webhooks.rs", status: .untracked, added: 48, removed: 0), """
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

            """),
    ]

    private static let reset = "\u{1b}[0m"
    private static let dim = "\u{1b}[2m"
    private static let bold = "\u{1b}[1m"
    private static let green = "\u{1b}[32m"
    private static let yellow = "\u{1b}[33m"
    private static let blue = "\u{1b}[34m"
    private static let magenta = "\u{1b}[35m"
    private static let cyan = "\u{1b}[36m"
    private static let removedLine = "\u{1b}[48;5;52m"
    private static let addedLine = "\u{1b}[48;5;22m"

    static func screen(for p: Pane) -> Data {
        let lines: [String]
        switch p.role {
        case .agent: lines = agentScreen
        case .shell: lines = shellScreen
        default: lines = editorScreen
        }
        // Rows end with CRLF: the terminal does not add a carriage return on its own.
        return Data(("\u{1b}c" + lines.joined(separator: "\r\n")).utf8)
    }

    private static var agentScreen: [String] {
        let box = String(repeating: "─", count: Int(cols) - 2)
        return [
            "\(bold)> \(reset)Handle Stripe's invoice.paid webhook. Mark the invoice paid and email a receipt.",
            "",
            "\(green)●\(reset) I'll look at how the other webhooks are routed first.",
            "",
            "\(green)●\(reset) \(bold)Grep\(reset)(\"event.type\" in src/webhooks)",
            "  \(dim)⎿  src/webhooks/router.rs:42\(reset)",
            "",
            "\(green)●\(reset) \(bold)Read\(reset)(src/webhooks/router.rs)",
            "  \(dim)⎿  Read 118 lines\(reset)",
            "",
            "\(green)●\(reset) \(bold)Update\(reset)(src/webhooks/router.rs)",
            "  \(dim)⎿  Updated src/webhooks/router.rs with 9 additions and 2 removals\(reset)",
            "      \(dim)40\(reset)          \"customer.created\" => customers::upsert(&mut tx, &event.data).await?,",
            "      \(dim)41\(reset) \(removedLine)-        \"invoice.paid\" => return Err(WebhookError::Unhandled(event.kind)),\(reset)",
            "      \(dim)41\(reset) \(addedLine)+        \"invoice.paid\" => {\(reset)",
            "      \(dim)42\(reset) \(addedLine)+            let invoice = Invoice::paid(&mut tx, &event.data.id).await?;\(reset)",
            "      \(dim)43\(reset) \(addedLine)+            jobs::enqueue(&mut tx, Job::Receipt(invoice.id)).await?;\(reset)",
            "      \(dim)44\(reset) \(addedLine)+        }\(reset)",
            "",
            "\(green)●\(reset) \(bold)Bash\(reset)(cargo test -p billing webhooks)",
            "  \(dim)⎿  test result: \(reset)\(green)ok\(reset)\(dim). 14 passed; 0 failed; finished in 2.31s\(reset)",
            "",
            "\(green)●\(reset) All 14 webhook tests pass. Stripe can send the same event twice.",
            "  Should I store the event id to skip duplicates?",
            "",
            "  \(cyan)❯ 1. Yes, add an events table\(reset)",
            "    2. No, the update is idempotent already",
            "",
            "\(dim)╭\(box)╮\(reset)",
            "\(dim)│\(reset) > " + String(repeating: " ", count: Int(cols) - 5) + "\(dim)│\(reset)",
            "\(dim)╰\(box)╯\(reset)",
            "  \(dim)? for shortcuts\(reset)",
        ]
    }

    private static var shellScreen: [String] {
        let prompt = "\(blue)billing\(reset) on \(magenta) feat/stripe-webhooks\(reset) \(yellow)!\(reset)"
        return [
            prompt,
            "\(green)❯\(reset) git status --short",
            " \(yellow)M\(reset) src/invoice.rs",
            " \(yellow)M\(reset) src/webhooks/router.rs",
            "\(dim)??\(reset) tests/webhooks.rs",
            "",
            prompt,
            "\(green)❯\(reset) cargo test -p billing",
            "   \(green)\(bold)Compiling\(reset) billing v0.4.0 (~/src/.worktrees/billing-stripe-webhooks)",
            "    \(green)\(bold)Finished\(reset) `test` profile [unoptimized + debuginfo] target(s) in 6.42s",
            "     \(green)\(bold)Running\(reset) tests/webhooks.rs",
            "",
            "running 14 tests",
            "test customer_created_upserts_the_customer ... \(green)ok\(reset)",
            "test duplicate_event_is_accepted ... \(green)ok\(reset)",
            "test invoice_paid_marks_the_invoice_and_queues_a_receipt ... \(green)ok\(reset)",
            "test invoice_payment_failed_queues_a_notice ... \(green)ok\(reset)",
            "test signature_with_clock_skew_is_accepted ... \(green)ok\(reset)",
            "test unknown_event_is_rejected ... \(green)ok\(reset)",
            "test wrong_signature_is_rejected ... \(green)ok\(reset)",
            "\(dim)…\(reset)",
            "",
            "test result: \(green)ok\(reset). 14 passed; 0 failed; 0 ignored; finished in 2.31s",
            "",
            prompt,
            "\(green)❯\(reset) ",
        ]
    }

    private static var editorScreen: [String] {
        [
            "\(bold)\(magenta)# Week 43\(reset)",
            "",
            "- [x] Stripe webhooks: plan reviewed",
            "- [ ] Retry backoff: check the jitter",
            "- [ ] Dark mode: review screenshots",
            "",
            "\(dim)~\(reset)",
            "\(dim)~\(reset)",
        ]
    }
}
