package tech.xdge.muxa.demo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentRun
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.CaptureState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.BranchChanges
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.PROTOCOL_VERSION
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WORK_ASK
import tech.xdge.muxa.protocol.WORK_ASSISTANT
import tech.xdge.muxa.protocol.WORK_ASK_AGENT
import tech.xdge.muxa.protocol.WORK_DELETE_INBOX
import tech.xdge.muxa.protocol.WORK_CAPABILITY
import tech.xdge.muxa.protocol.WORK_CLOSE_ASK
import tech.xdge.muxa.protocol.WORK_ITEMS
import tech.xdge.muxa.protocol.WORK_PLANS
import tech.xdge.muxa.protocol.WORK_POLICIES
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.PlanAction
import tech.xdge.muxa.protocol.WORK_ROUTINES
import tech.xdge.muxa.protocol.WORK_WORKTREE_END
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkspaceInfo
import tech.xdge.muxa.protocol.WorktreeEnd
import tech.xdge.muxa.transport.BridgeChannel
import java.util.UUID

/**
 * An in-app bridge for a demo without a host: a few sessions, typed text echoed back,
 * and a chat for the agent pane %1. Most panes have a fixed size; the content's
 * followers, and new tasks, take the app's viewport as a host with `window-size latest`
 * does (ADR 0020). No SSH, no tmux. With [alerts], an agent elsewhere starts waiting a
 * few seconds in.
 */
/**
 * [taskRecords] false plays a bridge from before task records, for the old New Task sheet's test.
 * [workLevel] is the `work` level it advertises; at [WORK_POLICIES] it keeps policies, corrections and hypotheses
 * (ADR 0050); at [WORK_PLANS] it has the overall assistant's plans (ADR 0049);
 * at [WORK_ASSISTANT] it keeps Captures and Work items (ADR 0048); below [WORK_DELETE_INBOX] it plays a host from before
 * Protect and Delete Old (ADR 0046), below [WORK_WORKTREE_END] one whose Done keeps the worktree (ADR 0043),
 * below [WORK_ASK_AGENT] one from before Ask Agent (ADR 0041), below [WORK_CLOSE_ASK] one from before closing
 * ask sessions (ADR 0039), below [WORK_ITEMS] one from before
 * Inbox items (ADR 0033), below [WORK_ASK] one from before ask sessions and branches named after
 * the title (ADR 0031), and at 0 one without Routines. [oldItems] makes the failure and the last
 * proposal four days old, for Delete Old's UI test (ADR 0046), as the iOS demo's `-old-items` does.
 */
class DemoBridge(
    set: Content = Content.TESTS,
    alerts: Boolean = false,
    taskRecords: Boolean = true,
    workLevel: Int = WORK_DELETE_INBOX,
    oldItems: Boolean = false,
    /** How fast an assistant run moves on (ADR 0048). */
    assistPace: AssistPace = AssistPace.DEMO,
) : BridgeChannel {
    /** [TESTS] holds the fixtures the UI tests assert on; [SHOWCASE] looks like real work. */
    enum class Content { TESTS, SHOWCASE }

    private val content: DemoContent = when (set) {
        Content.TESTS -> TestContent
        Content.SHOWCASE -> ShowcaseContent
    }
    private val out = Channel<String>(Channel.UNLIMITED)
    override val lines: ReceiveChannel<String> = out
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val lock = Any()
    private var watch = 0L
    private var watchedPane: String? = null

    /** The app's grid at its default font, once it sent one. */
    private var viewport: Pair<Int, Int>? = null

    /** Panes whose window follows the device in use. */
    private val followers = content.followers.toMutableSet()
    private var chat: ChatFollow? = null
    private var openDoc: Long? = null
    private var openDiff: Long? = null

    /** The last paste into %1, sent as a prompt when Enter follows. */
    private var prompt: String? = null

    /** What the demo agent waits on, and the tool call it waits on. */
    private var waitingOn: Pair<ChatPrompt, ChatEntry>? = null

    /** The texts Inbox answers sent to the agent of %1, after its conversation. */
    private var itemPrompts: List<ChatEntry> = emptyList()

    /** Grows when a task starts. */
    /** Routines and stored Inbox entries (ADR 0026). */
    private val work = content.work(now())

    /** The Tasks' Inbox items (ADR 0033). */
    private val items = content.items(now())

    /** Captures and Work items (ADR 0048). */
    private val assistant = content.assistant(now()).also { it.pace = assistPace }
    private var sessions: List<Session> = content.sessions(now()) + work.sessions
    private val capabilities = CAPABILITIES
        .filter { taskRecords || it != "task-records" } +
        listOfNotNull("$WORK_CAPABILITY:$workLevel".takeIf { workLevel > 0 })

    private val workLevel = tech.xdge.muxa.protocol.workLevel(capabilities)

    /** The overall assistant (ADR 0049): its pins, deadlines and proposal items join the fixtures at level 10. */
    private val plans: DemoPlans? = content.plans(now())?.takeIf { workLevel >= WORK_PLANS }?.also { p ->
        assistant.seed(p.pins, p.dues)
        items.add(p.proposals)
        p.planMillis = assistPace.pendingMillis + assistPace.processingMillis
    }

    /** Policies, corrections and hypotheses (ADR 0050): the plan's rows cite them, and the hypotheses ask in the Inbox at level 11. */
    private val policies: DemoPolicies? = plans?.let { p ->
        content.policies(now())?.takeIf { workLevel >= WORK_POLICIES }?.also { m ->
            p.cite(m.cites)
            items.add(m.items(p.overall))
        }
    }

    /** The workspaces `list-tasks` names: the overall assistant's is one of them. */
    private val listedWorkspaces = content.workspaces + listOfNotNull(plans?.let { WorkspaceInfo(it.overall) })
    private val images = content.images(now())

    /** The host's task records; grows when a task is created. */
    private var tasks: List<TaskRecord> = content.tasks(now())
    private var createdCount = 0
    private var runCount = 0

    private class ChatFollow(val id: Long, var seq: Long = 0, var count: Int = 0)

    init {
        if (oldItems) {
            val old = now() - OLD_ITEM_AGE
            items.age("i-0000a003", old)
            work.age("e-000000a3", old - 60)
        }
        if (alerts) {
            scope.launch {
                delay(content.alertDelayMillis)
                agentWaits("$4")
            }
        }
        emit(Message.Hello(PROTOCOL_VERSION, "demo", "demo", capabilities))
    }

    override suspend fun send(line: String) {
        when (val req = Wire.decodeRequest(line)) {
            is Request.List -> emit(Message.Result(req.id, sessions = synchronized(lock) { sessions }))
            is Request.Watch -> watch(req)
            is Request.History -> history(req)
            is Request.Viewport -> viewport(req)
            is Request.Text -> {
                ok(req.id)
                echo(req.pane, req.text.toByteArray())
            }
            is Request.Paste -> {
                ok(req.id)
                if (req.pane == CHAT_PANE) synchronized(lock) { prompt = req.text }
                echo(req.pane, req.text.replace("\n", "\r\n").toByteArray())
            }
            is Request.Chat -> chat(req)
            is Request.Unchat -> {
                synchronized(lock) { chat = null }
                ok(req.id)
            }
            is Request.Docs -> {
                // The bridge lists a Task's TASK.md, its HANDOFF.md, and its CHECK.md once a check ran.
                val brief = taskOf(req.pane)?.let {
                    listOfNotNull(
                        DocInfo(TASK_DOC, DocInfo.Kind.REPO, "TASK.md", it.createdAt),
                        DocInfo(HANDOFF_DOC, DocInfo.Kind.REPO, "HANDOFF.md", handoffTime(it)),
                        it.loop?.lastCheck?.let { check -> DocInfo(CHECK_DOC, DocInfo.Kind.REPO, "CHECK.md", check.at) },
                    )
                }
                emit(Message.DocList(req.id, content.root, content.docs.map { it.info } + brief.orEmpty()))
                ok(req.id)
            }
            is Request.CreateTask -> scope.launch {
                // A launch takes a moment on a real host too.
                delay(1_000)
                createTask(req)
            }
            is Request.StartSession -> startSession(req)
            is Request.UpdateTask -> updateTask(req)
            is Request.RecoverRestart -> fail(req.id, ErrorCode.UNSUPPORTED, "Restart recovery is not implemented on this host.")
            is Request.RestartAgent -> restartAgent(req)
            is Request.StartLoop -> startLoop(req)
            is Request.StopLoop -> stopLoop(req)
            is Request.ListTasks -> {
                emit(Message.TaskList(req.id, listedWorkspaces, synchronized(lock) { tasks }))
                ok(req.id)
            }
            is Request.Doc -> doc(req)
            is Request.Undoc -> {
                synchronized(lock) { openDoc = null }
                ok(req.id)
            }
            is Request.NewTask -> scope.launch {
                // A launch takes a moment on a real host too.
                delay(1_000)
                startTask(req.id, req.repo, req.branch)
            }
            is Request.Changes -> {
                val branch = BranchChanges("origin/main", 3, content.branch.map { it.info })
                emit(Message.ChangeList(req.id, content.root, content.changes.map { it.info }, branch = branch))
                ok(req.id)
            }
            is Request.Diff -> if (req.scope == DiffScope.BRANCH) branchDiff(req) else diff(req)
            is Request.Undiff -> {
                synchronized(lock) { openDiff = null }
                ok(req.id)
            }
            is Request.Images -> {
                emit(Message.ImageList(req.id, content.root, images.map { it.info }))
                ok(req.id)
            }
            is Request.Image -> image(req)
            is Request.Unimage -> ok(req.id)
            is Request.CloseTask -> closeTask(req.id, req.session, req.mode)
            is Request.Keys -> keys(req)
            is Request.ListRoutines, is Request.ListInbox, is Request.RunRoutine, is Request.UpdateInbox,
            is Request.DeleteInbox -> serveWork(req)
            is Request.Capture, is Request.SortCapture, is Request.UpdateWork, is Request.GetWork,
            is Request.RequestAssistant, is Request.CancelRequest -> serveAssistant(req)
            is Request.ApplyNotes, is Request.PlanNow -> servePlans(req)
            is Request.AddPolicy, is Request.UpdatePolicy, is Request.UpdatePlan, is Request.Forget -> servePolicies(req)
            is Request.Ping, is Request.Unwatch, is Request.RawBytes -> ok(req.id)
            is Request.Bye -> out.close()
        }
    }

    /** Routines and stored entries, refused as a bridge without `work` level 1 refuses them. */
    private fun serveWork(req: Request) {
        if (workLevel < WORK_ROUTINES) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        val now = now()
        when (req) {
            is Request.ListRoutines -> {
                emit(Message.RoutineList(req.id, synchronized(lock) { work.routines }))
                ok(req.id)
            }
            is Request.ListInbox -> {
                if (workLevel >= WORK_ASSISTANT) advanceRuns(now)
                val list = synchronized(lock) {
                    val entries = work.shownEntries
                    val shown = if (workLevel >= WORK_ITEMS) items.list(entries, work.laterAts, workLevel >= WORK_ASK_AGENT) else emptyList()
                    if (workLevel < WORK_ASSISTANT) return@synchronized Message.InboxList(req.id, entries, shown)
                    val listed = assistant.listed(::derivedState)
                    val p = plans ?: return@synchronized Message.InboxList(req.id, entries, shown, listed, assistant.unsorted, assistant.requestsOf(listed))
                    p.advance(now, System.currentTimeMillis(), listed, shown)
                    val (unanswered, stale) = assistant.unansweredAndStale()
                    Message.InboxList(
                        req.id, entries, shown, listed, assistant.unsorted, assistant.requestsOf(listed),
                        p.listed(unanswered, stale), p.info,
                        policies?.listedPolicies(now).orEmpty(), policies?.listedHypotheses.orEmpty(), policies?.listedCorrections.orEmpty(),
                    )
                }
                emit(list)
                ok(req.id)
            }
            is Request.RunRoutine -> runRoutine(req, now)
            is Request.DeleteInbox -> deleteInbox(req, now)
            is Request.UpdateInbox -> if (req.action == InboxAction.ASK_AGENT) {
                askAgent(req)
            } else if (req.action in PROTECT_ACTIONS) {
                protect(req, now)
            } else if (synchronized(lock) { items.item(req.entry)?.kind } == ItemKind.HYPOTHESIS) {
                answerHypothesis(req, now)
            } else if (synchronized(lock) { items.item(req.entry)?.kind }.let { it == ItemKind.SORT || it == ItemKind.DELEGATE }) {
                answerProposal(req, now)
            } else if (workLevel >= WORK_ITEMS && synchronized(lock) { items.has(req.entry) }) answerItem(req, now) else updateEntry(req, now)
            else -> fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        }
    }

    /** Captures, Work items and assistant requests (ADR 0048), refused as a bridge below `work` level 9 refuses them. */
    private fun serveAssistant(req: Request) {
        if (workLevel < WORK_ASSISTANT) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        val now = now()
        try {
            val saved = synchronized(lock) {
                when (req) {
                    is Request.Capture -> return capture(req, now)
                    is Request.SortCapture -> assistant.sort(req, now).let { (c, w) -> Message.CaptureSaved(req.id, c, w?.let(::derived)) }
                    is Request.UpdateWork -> {
                        if (workLevel < WORK_PLANS && (req.action == WorkAction.PIN || req.action == WorkAction.UNPIN || req.due != null)) {
                            return fail(req.id, ErrorCode.BAD_REQUEST, "pin, unpin and due need work level 10")
                        }
                        val laterAt = maxOf(now, (assistant.lastLaterAt ?: 0) + 1)
                        val saved = assistant.update(req, now, laterAt)
                        // Later, Pin and Unpin are the host's own observations; one that changes nothing records nothing.
                        WORK_CORRECTIONS[req.action]?.takeIf { saved.rev != req.rev }?.let { observe(it, saved.id, saved.workspace, now) }
                        Message.WorkSaved(req.id, derived(saved))
                    }
                    is Request.GetWork -> assistant.detail(req.work).let { (w, captures, requests) ->
                        Message.WorkDetail(req.id, derived(w), captures, requests)
                    }
                    is Request.RequestAssistant -> return ask(req, now)
                    is Request.CancelRequest -> {
                        val (w, q, ended) = assistant.cancel(req, now)
                        ended?.let(::endRun)
                        Message.WorkSaved(req.id, derived(w), q)
                    }
                    else -> return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
                }
            }
            emit(saved)
            ok(req.id)
            if (req is Request.CancelRequest) emit(Message.SessionsChanged)
        } catch (e: DemoWork.Refusal) {
            fail(req.id, e.code, e.message ?: "")
        }
    }

    /** A Capture; with `assist`, also a Work item and a request. A refusal still saves the Capture. The caller holds the lock. */
    private fun capture(req: Request.Capture, now: Long) {
        val c = assistant.capture(req, now)
        val spec = req.assist ?: return emitAll(Message.CaptureSaved(req.id, c), Message.Result(req.id))
        val a = assistant.assist(c, spec, now, System.currentTimeMillis(), workspaceNames, ::startRun)
        a.asked?.ended?.let(::endRun)
        emit(Message.CaptureSaved(req.id, a.capture, a.work?.let(::derived), a.asked?.request))
        val refusal = a.refusal
        if (refusal != null) return fail(req.id, refusal.code, refusal.message ?: "")
        ok(req.id)
        if (a.asked?.run != null) runStarted()
    }

    /** `request-assistant`: a run with its own session, as the daemon starts one. The caller holds the lock. */
    private fun ask(req: Request.RequestAssistant, now: Long) {
        val a = assistant.ask(req, now, System.currentTimeMillis(), workspaceNames, ::startRun)
        a.ended?.let(::endRun)
        emitAll(Message.WorkSaved(req.id, derived(a.work), a.request), Message.Result(req.id))
        if (a.run != null) runStarted()
    }

    /** The workspaces that take requests: not the overall assistant's. */
    private val workspaceNames get() = content.workspaces.map { it.name }.toSet()

    /** `plan-now` and `apply-notes` (ADR 0049), refused as a bridge below `work` level 10 refuses them. */
    private fun servePlans(req: Request) {
        val p = plans ?: return fail(req.id, ErrorCode.BAD_REQUEST, "unknown request for work level $workLevel")
        val now = now()
        try {
            val saved = synchronized(lock) {
                when (req) {
                    is Request.PlanNow -> Message.PlanSaved(req.id, p.planNow(req.minutes, now, System.currentTimeMillis()))
                    is Request.ApplyNotes -> if (req.source.startsWith("p-")) {
                        Message.PlanSaved(req.id, p.applyPlanNotes(req.source, req.version, req.base, now))
                    } else {
                        val q = assistant.request(req.source) ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no request ${req.source}")
                        val result = q.results.firstOrNull { it.version == req.version && !it.stale }
                        p.applyNotes(q.workspace, result?.notes, req.base, now)
                        Message.WorkSaved(req.id, derived(assistant.workItem(q.work) ?: return@synchronized null), q)
                    }
                    else -> null
                }
            } ?: return fail(req.id, ErrorCode.BAD_REQUEST, NO_REQUEST)
            emitAll(saved, Message.Result(req.id))
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        if (req is Request.PlanNow) planStarted(p.planMillis)
    }

    /**
     * `add-policy`, `update-policy`, `update-plan` and `forget` (ADR 0050), refused as a bridge below
     * `work` level 11 refuses them.
     */
    private fun servePolicies(req: Request) {
        val m = policies ?: return fail(req.id, ErrorCode.BAD_REQUEST, "unknown request for work level $workLevel")
        val p = plans ?: return fail(req.id, ErrorCode.BAD_REQUEST, NO_REQUEST)
        val now = now()
        try {
            val saved = synchronized(lock) {
                when (req) {
                    is Request.AddPolicy -> Message.PolicySaved(req.id, m.add(req, now, workspaceNames))
                    is Request.UpdatePolicy -> m.update(req, now, workspaceNames)?.let { Message.PolicySaved(req.id, it) }
                    is Request.UpdatePlan -> updatePlan(p, m, req, now)
                    is Request.Forget -> {
                        val gone = m.forget(req.record)
                        items.remove(gone.map { DemoPolicies.itemId(it) }.toSet())
                        null
                    }
                    else -> null
                }
            }
            saved?.let(::emit)
            ok(req.id)
        } catch (e: DemoWork.Refusal) {
            fail(req.id, e.code, e.message ?: "")
        }
    }

    /** Reorder or Not today on the current plan; a change records one correction. The caller holds the lock. */
    private fun updatePlan(p: DemoPlans, m: DemoPolicies, req: Request.UpdatePlan, now: Long): Message {
        req.text?.let(m::checkReason)
        val before = p.current?.let { c -> c.userOrder.ifEmpty { c.top.mapNotNull { it.work ?: it.item } } }.orEmpty()
        val (plan, changed) = p.update(req)
        if (changed) {
            when (req.action) {
                PlanAction.REORDER -> m.record(CorrectionKind.REORDERED, plan.id, null, true, null, now, before = before, after = req.order)
                PlanAction.DISMISS -> {
                    val row = req.row.orEmpty()
                    m.record(CorrectionKind.DISMISSED, plan.id, row, true, workspaceOf(row), now, req.text)
                }
            }
        }
        return Message.PlanSaved(req.id, plan)
    }

    /** The workspace of a Work item or Inbox item; null for nobody's. The caller holds the lock. */
    private fun workspaceOf(row: String): String? = assistant.workItem(row)?.workspace ?: items.item(row)?.workspace

    /** Records a correction while a plan is current (ADR 0050 §2); nothing below level 11. The caller holds the lock. */
    private fun observe(kind: CorrectionKind, row: String, workspace: String?, now: Long, reason: String? = null) {
        val m = policies ?: return
        val plan = plans?.current ?: return
        m.record(kind, plan.id, row, plan.top.any { (it.work ?: it.item) == row }, workspace, now, reason)
    }

    /**
     * Confirm, Reject or Later on a `hypothesis` item (ADR 0050 §3). Confirm makes a policy first, then
     * answers the item; a second confirm returns the same policy. Reject records a correction with the reason.
     */
    private fun answerHypothesis(req: Request.UpdateInbox, now: Long) {
        val m = policies ?: return fail(req.id, ErrorCode.BAD_REQUEST, NO_REQUEST)
        try {
            val (policy, item) = synchronized(lock) {
                val item = items.item(req.entry) ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "no Inbox item ${req.entry}")
                val h = item.hypothesis?.hypothesis ?: throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${item.id} has no hypothesis")
                if (item.answer != InboxAction.CONFIRM && req.action != InboxAction.READ && req.action !in item.answers) {
                    throw DemoWork.Refusal(ErrorCode.BAD_REQUEST, "${req.action} does not answer Inbox item ${item.id} now")
                }
                if (req.action == InboxAction.REJECT) req.text?.let(m::checkReason)
                val policy = if (req.action == InboxAction.CONFIRM) m.confirm(h, now) else null
                val answered = if (item.answer == InboxAction.CONFIRM) item else items.answer(item.id, req.action, null, null, now, nextLaterAt(now)).item
                if (req.action == InboxAction.REJECT) {
                    m.reject(h, now)
                    observe(CorrectionKind.REJECTED, item.id, item.workspace, now, req.text)
                }
                policy to answered
            }
            policy?.let { emit(Message.PolicySaved(req.id, it)) }
            emit(Message.InboxSaved(req.id, item = item))
            ok(req.id)
        } catch (e: DemoWork.Refusal) {
            fail(req.id, e.code, e.message ?: "")
        }
    }

    /** The app lists again once the plan should be made, as the host's `tasks-changed` makes it. */
    private fun planStarted(after: Long) {
        if (assistPaceNever) return
        scope.launch {
            delay(after + RUN_TICK_MILLIS)
            emit(Message.TasksChanged)
        }
    }

    private val assistPaceNever get() = assistant.pace == AssistPace.NEVER

    /**
     * Approve, Reject or Later on a `sort` or `delegate` item (ADR 0049 §4). Approve runs the action
     * once, at the Capture's or Work item's current `rev`; a second approve returns the item. A
     * refused delegate leaves the item open; a Capture sorted meanwhile resolves it.
     */
    private fun answerProposal(req: Request.UpdateInbox, now: Long) {
        val outcome = synchronized(lock) {
            val item = items.item(req.entry) ?: return@synchronized ProposalOutcome(null, emptyList(), ErrorBody(ErrorCode.BAD_REQUEST, "no item"))
            if (req.action != InboxAction.APPROVE || item.answer == InboxAction.APPROVE) {
                if (item.answer == InboxAction.APPROVE) return@synchronized ProposalOutcome(item, emptyList(), null)
                return@synchronized try {
                    if (req.action == InboxAction.REJECT) req.text?.let { policies?.checkReason(it) }
                    val answered = items.answer(item.id, req.action, null, null, now, nextLaterAt(now)).item
                    if (req.action == InboxAction.REJECT) observe(CorrectionKind.REJECTED, item.id, item.workspace, now, req.text)
                    ProposalOutcome(answered, emptyList(), null)
                } catch (e: DemoWork.Refusal) {
                    ProposalOutcome(null, emptyList(), ErrorBody(e.code, e.message ?: ""))
                }
            }
            approve(item, now, req.id)
        }
        outcome.written.forEach(::emit)
        outcome.item?.let { emit(Message.InboxSaved(req.id, item = it)) }
        outcome.error?.let { fail(req.id, it.code, it.message) } ?: ok(req.id)
        if (outcome.started) runStarted()
    }

    private class ProposalOutcome(val item: InboxItem?, val written: List<Message>, val error: ErrorBody?, val started: Boolean = false)

    /** Runs an approved proposal's action. The caller holds the lock. */
    private fun approve(item: InboxItem, now: Long, id: Long): ProposalOutcome {
        val answered = { summary: String? ->
            item.copy(state = ItemState.ANSWERED, answer = InboxAction.APPROVE, answeredAt = now, laterAt = null, answers = emptyList(), summary = summary)
                .also(items::save)
        }
        item.sort?.let { s ->
            val c = assistant.captureOf(s.capture)
            if (c == null || c.state == CaptureState.SORTED) {
                val resolved = item.copy(state = ItemState.RESOLVED, answers = emptyList(), summary = "${s.capture} was sorted meanwhile.").also(items::save)
                return ProposalOutcome(resolved, emptyList(), ErrorBody(ErrorCode.CONFLICT, "${s.capture} was sorted meanwhile"))
            }
            val (sorted, w) = assistant.sort(Request.SortCapture(0, c.id, c.rev, s.action, s.work, s.title, s.workspace), now)
            return ProposalOutcome(answered(null), listOf(Message.CaptureSaved(id, sorted, w?.let(::derived))), null)
        }
        val d = item.delegate ?: return ProposalOutcome(null, emptyList(), ErrorBody(ErrorCode.BAD_REQUEST, "${item.id} has no action"))
        val w = assistant.workItem(d.work) ?: return ProposalOutcome(null, emptyList(), ErrorBody(ErrorCode.BAD_REQUEST, "no Work item ${d.work}"))
        return try {
            val a = assistant.ask(Request.RequestAssistant(0, w.id, w.rev, d.scope, d.workspace, d.purpose), now, System.currentTimeMillis(), workspaceNames, ::startRun)
            a.ended?.let(::endRun)
            ProposalOutcome(answered(null), listOf(Message.WorkSaved(id, derived(a.work), a.request)), null, started = a.run != null)
        } catch (e: DemoWork.Refusal) {
            ProposalOutcome(null, emptyList(), ErrorBody(e.code, e.message ?: ""))
        }
    }

    /** A new run's session, named `<scope>-<request>-v<version>-<run>` as the host names it. The caller holds the lock. */
    private fun startRun(name: String): String {
        val id = "t-run%03d".format(++runCount)
        val n = 20 + sessions.size
        val dir = "/assistant/$id"
        val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", dir, false, false, Agent(AgentState.RUNNING, now()))
        followers += pane.id
        sessions = sessions + Session("$$n", "$name-$id", taskId = id, panes = listOf(pane))
        return id
    }

    /** The run's session closes. The caller holds the lock. */
    private fun endRun(run: String) {
        sessions = sessions.filter { it.taskId != run }
    }

    /** The app lists again when the run should have moved on, as a host's `tasks-changed` makes it. */
    private fun runStarted() {
        emit(Message.SessionsChanged)
        val pace = assistant.pace
        if (pace == AssistPace.NEVER) return
        scope.launch {
            delay(pace.pendingMillis + RUN_TICK_MILLIS)
            emit(Message.TasksChanged)
            delay(pace.processingMillis)
            emit(Message.TasksChanged)
        }
    }

    /** Moves the runs on before a list; a run with its Result closes its session. */
    private fun advanceRuns(now: Long) {
        val ended = synchronized(lock) {
            assistant.advance(now, System.currentTimeMillis()).onEach(::endRun)
        }
        if (ended.isNotEmpty()) emit(Message.SessionsChanged)
    }

    private fun emitAll(vararg messages: Message) = messages.forEach(::emit)

    /** [w] with the state the host derives. The caller holds the lock. */
    private fun derived(w: WorkItem) = w.copy(state = derivedState(w))

    /**
     * What the host derives for an open Work item: waiting on a decision while a linked Task has
     * an open Inbox item, in progress while one runs. The caller holds the lock.
     */
    private fun derivedState(w: WorkItem): WorkState {
        if (w.state != WorkState.OPEN) return w.state
        if (w.tasks.isEmpty()) return if (assistant.running(w)) WorkState.IN_PROGRESS else w.state
        val asking = items.list(work.shownEntries, work.laterAts, askAgent = false)
            .filter { it.state == ItemState.OPEN }.mapNotNull { it.task }.toSet()
        if (w.tasks.any { it in asking }) return WorkState.WAITING_DECISION
        val running = tasks.filter { it.id in w.tasks && it.status == TaskStatus.OPEN }.map { it.id }
        return if (sessions.any { it.taskId in running } || assistant.running(w)) WorkState.IN_PROGRESS else WorkState.OPEN
    }

    /**
     * Ask Agent (ADR 0041): the workspace's one Ask session at its root, started on the first
     * request and reused after. The item stays open.
     */
    private fun askAgent(req: Request.UpdateInbox) {
        if (workLevel < WORK_ASK_AGENT) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        if (req.text.isNullOrEmpty()) return fail(req.id, ErrorCode.BAD_REQUEST, "ask-agent needs a text")
        val found = synchronized(lock) {
            val item = items.list(work.shownEntries, work.laterAts, askAgent = true)
                .firstOrNull { it.id == req.entry && it.state == ItemState.OPEN && InboxAction.ASK_AGENT in it.answers }
                ?: return@synchronized null
            val name = "ask-${item.workspace}"
            val open = sessions.firstOrNull { it.name == name }
            if (open != null) return@synchronized Triple(item, open, false)
            val root = content.workspaces.firstOrNull { it.name == item.workspace }?.root ?: "/"
            val n = 20 + sessions.size
            val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", root, false, false, Agent(AgentState.RUNNING, now()))
            followers += pane.id
            val session = Session("$$n", name, ask = item.workspace, panes = listOf(pane))
            sessions = sessions + session
            Triple(item, session, true)
        } ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.entry} is not an open item that takes ask-agent")
        val (item, session, started) = found
        emit(Message.SessionStarted(req.id, session.id, session.name))
        emit(Message.InboxSaved(req.id, item = item))
        ok(req.id)
        if (started) emit(Message.SessionsChanged)
    }

    /** Protect and Unprotect (ADR 0046): on a Task's item or a Routine's entry, open or closed. */
    private fun protect(req: Request.UpdateInbox, now: Long) {
        if (workLevel < WORK_DELETE_INBOX) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        val at = now.takeIf { req.action == InboxAction.PROTECT }
        val (entry, item) = try {
            synchronized(lock) {
                if (items.has(req.entry)) {
                    null to items.protect(req.entry, at)
                } else {
                    val saved = work.protect(req.entry, at)
                    saved to DemoItems.fromEntry(saved, work.laterAts[saved.id], askAgent = true)
                }
            }
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        emit(Message.InboxSaved(req.id, entry, item))
        ok(req.id)
    }

    /** Delete Old (ADR 0046): every item and entry made more than the days ago, except protected ones. */
    private fun deleteInbox(req: Request.DeleteInbox, now: Long) {
        if (workLevel < WORK_DELETE_INBOX) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        if (req.olderThanDays !in 1..MAX_DELETE_DAYS) {
            return fail(req.id, ErrorCode.BAD_REQUEST, "olderThanDays must be 1 to $MAX_DELETE_DAYS")
        }
        val cutoff = now - req.olderThanDays * DAY_SECONDS
        val deleted = synchronized(lock) { items.deleteBefore(cutoff) + work.deleteBefore(cutoff) }
        emit(Message.InboxDeleted(req.id, deleted))
        ok(req.id)
    }

    /** Starts a run with a session whose agent runs, as the daemon does. */
    private fun runRoutine(req: Request.RunRoutine, now: Long) {
        val saved = try {
            synchronized(lock) {
                val (routine, session) = work.run(req.workspace, req.routine, 20 + sessions.size, now)
                sessions = sessions + session
                routine
            }
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        emit(Message.RoutineSaved(req.id, saved))
        ok(req.id)
        emit(Message.SessionsChanged)
    }

    /** Approve creates one Task, as `create-task` would, and nothing on a second approval; dismiss and later touch no Task. */
    private fun updateEntry(req: Request.UpdateInbox, now: Long) {
        val (saved, created) = try {
            synchronized(lock) {
                val update = work.update(req.entry, req.action, nextLaterAt(now))
                val entry = update.entry
                val goal = update.goal
                if (goal == null || entry.task != null) {
                    entry to false
                } else {
                    val project = work.routines.firstOrNull { it.workspace == entry.workspace && it.name == entry.routine }?.project
                    val record = approvedTask(entry.workspace, entry.title, goal, project, now)
                    work.approved(entry.id, record.id) to true
                }
            }
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        val item = if (workLevel >= WORK_ITEMS) DemoItems.fromEntry(saved, synchronized(lock) { work.laterAts[saved.id] }, workLevel >= WORK_ASK_AGENT) else null
        emit(Message.InboxSaved(req.id, saved, item))
        ok(req.id)
        if (created) emit(Message.SessionsChanged)
    }

    /**
     * What `later` sets: [now], or one second after the last one, so two in one second keep the
     * order they were moved in (ADR 0040). The caller holds the lock.
     */
    private fun nextLaterAt(now: Long): Long {
        val last = (work.laterAts.values + listOfNotNull(items.lastLaterAt)).maxOrNull() ?: return now
        return maxOf(now, last + 1)
    }

    /** A Task's item: Accept and Drop set the Task's status; the other answers touch no Task here. */
    private fun answerItem(req: Request.UpdateInbox, now: Long) {
        val saved = try {
            synchronized(lock) {
                if (DemoItems.ending(req.action) != null) {
                    lostWork(items.task(req.entry), req.worktree)?.let { throw DemoWork.Refusal(ErrorCode.WOULD_LOSE_WORK, it) }
                }
                val answer = items.answer(req.entry, req.action, req.text, req.option, now, nextLaterAt(now))
                // Later on an item the plan listed is a correction; it means "not now" only.
                if (req.action == InboxAction.LATER && plans?.current?.top?.any { it.item == req.entry } == true) {
                    observe(CorrectionKind.DEFERRED, req.entry, answer.item.workspace, now)
                }
                val status = answer.status
                if (status != null) {
                    tasks = tasks.map { if (it.id == answer.item.task) it.copy(rev = it.rev + 1, status = status, updatedAt = now) else it }
                }
                answer
            }
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        emit(Message.InboxSaved(req.id, item = DemoItems.shown(saved.item, workLevel >= WORK_ASK_AGENT)))
        ok(req.id)
        if (saved.status != null) emit(Message.TasksChanged)
        val text = req.text
        if (text != null && saved.item.pane == CHAT_PANE && saved.item.answer in PROMPT_ACTIONS) {
            val entry = synchronized(lock) {
                ChatEntry("u:item${itemPrompts.size}", ChatEntry.Kind.USER, text).also { itemPrompts = itemPrompts + it }
            }
            emitChat(listOf(entry), AgentState.IDLE)
        }
    }

    /** The Task an approval creates, with a running agent in its own session. The caller holds the lock. */
    private fun approvedTask(workspace: String, title: String, goal: String, project: String?, now: Long): TaskRecord {
        val id = "t-new%03d".format(++createdCount)
        val name = slug(title)
        val dir = "/$workspace/tasks/$name-$id"
        val record = TaskRecord(
            id, rev = 1, workspace = workspace, title = title, goal = goal, status = TaskStatus.OPEN,
            createdAt = now, updatedAt = now, project = project, dir = dir, session = name,
        )
        tasks = tasks + record
        val n = 20 + sessions.size
        val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", dir, false, false, Agent(AgentState.RUNNING, now))
        followers += pane.id
        sessions = sessions + Session("$$n", name, taskId = id, panes = listOf(pane))
        return record
    }

    override suspend fun close() {
        scope.cancel()
        out.close()
    }

    private fun watch(req: Request.Watch) {
        synchronized(lock) {
            watch = req.id
            watchedPane = req.pane
        }
        ok(req.id)
        val p = sized(req.pane) ?: return
        emit(Message.Screen(req.id, req.pane, p.cols, p.rows, Bytes(content.screen(p))))
    }

    /** Earlier output above the pane's screen, as the bridge sends it: one line per `\n`. */
    private fun history(req: Request.History) {
        val p = sized(req.pane) ?: return fail(req.id, ErrorCode.NO_PANE, "no pane ${req.pane}")
        val text = (1..HISTORY_LINES).joinToString("") { "Earlier output, line $it\n" }
        emit(Message.History(req.id, req.pane, p.cols, Bytes(text.toByteArray())))
        ok(req.id)
    }

    private fun viewport(req: Request.Viewport) {
        ok(req.id)
        val (w, pane) = synchronized(lock) {
            viewport = req.cols to req.rows
            watch to watchedPane
        }
        if (pane == null || synchronized(lock) { pane !in followers }) return
        val p = sized(pane) ?: return
        emit(Message.Screen(w, pane, p.cols, p.rows, Bytes(content.screen(p))))
    }

    /** The pane, at the viewport's size when its window follows the device. */
    private fun sized(id: String): Pane? = synchronized(lock) {
        val size = viewport?.takeIf { id in followers }
        if (size != null) {
            sessions = sessions.map { s ->
                s.copy(panes = s.panes.map { if (it.id == id) it.copy(cols = size.first, rows = size.second) else it })
            }
        }
        sessions.flatMap { it.panes }.firstOrNull { it.id == id }
    }

    private fun chat(req: Request.Chat) {
        if (req.pane != CHAT_PANE) {
            fail(req.id, ErrorCode.UNSUPPORTED, "no agent adapter knows this pane")
            return
        }
        ok(req.id)
        synchronized(lock) { chat = ChatFollow(req.id) }
        emitChat(content.conversation + synchronized(lock) { itemPrompts }, content.chatStatus, reset = true)
    }

    /** The record the session of [pane] runs. */
    private fun taskOf(pane: String): TaskRecord? = synchronized(lock) {
        val session = sessions.firstOrNull { s -> s.panes.any { it.id == pane } }
        tasks.firstOrNull { it.id == session?.taskId }
    }

    private fun doc(req: Request.Doc) {
        val task = taskOf(req.pane)
        if (req.path == TASK_DOC && task != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, brief(task), task.createdAt))
            return
        }
        if (req.path == HANDOFF_DOC && task != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, handoff(task), handoffTime(task)))
            return
        }
        val check = task?.loop?.lastCheck
        if (req.path == CHECK_DOC && check != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, checkLog(check), check.at))
            return
        }
        val doc = content.docs.firstOrNull { it.info.path == req.path }
        if (doc == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not listed")
            return
        }
        synchronized(lock) { openDoc = req.id }
        ok(req.id)
        emit(Message.DocContent(req.id, req.path, doc.text, 1))
        // The agent keeps writing its plan: an update arrives while it is open.
        val update = content.planUpdate
        if (doc.info.kind == DocInfo.Kind.PLAN && update != null) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDoc } != req.id) return@launch
                emit(Message.DocContent(req.id, req.path, doc.text + update, 2))
            }
        }
    }

    private fun branchDiff(req: Request.Diff) {
        val change = content.branch.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not in the branch")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        emit(Message.DiffContent(req.id, req.path, change.diff, binary = false))
    }

    private fun diff(req: Request.Diff) {
        val change = content.changes.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not changed")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        val diff = change.diff
        emit(Message.DiffContent(req.id, req.path, diff, binary = diff == null))
        // The agent keeps editing: the diff grows while it is open.
        val update = content.diffUpdate
        if (diff != null && update != null) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDiff } != req.id) return@launch
                emit(Message.DiffContent(req.id, req.path, diff + update, binary = false))
            }
        }
    }

    private fun image(req: Request.Image) {
        val image = images.firstOrNull { it.info.path == req.path }
        if (image == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not listed")
            return
        }
        ok(req.id)
        emit(Message.ImageContent(req.id, req.path, Bytes(Png.screenshot(image.color)), image.info.mtime))
    }

    private fun keys(req: Request.Keys) {
        ok(req.id)
        if (req.pane == CHAT_PANE && req.keys == listOf("Enter")) {
            val text = synchronized(lock) { prompt.also { prompt = null } }
            if (text != null) answer(text)
        }
        if (req.pane == CHAT_PANE) pick(req.keys)
        for (key in req.keys) {
            when (key) {
                "Enter" -> echo(req.pane, "\r\n$ ".toByteArray())
                "BSpace" -> echo(req.pane, "\b \b".toByteArray())
                else -> echo(req.pane, "\u001b[7m<$key>\u001b[0m".toByteArray())
            }
        }
    }

    private fun startTask(id: Long, repo: String, branch: String) {
        if (branch == FAILING_BRANCH) {
            fail(id, ErrorCode.TASK_FAILED, "could not create a worktree for $branch: a branch named '$branch' already exists")
            return
        }
        val name = branch.replace("/", "-")
        synchronized(lock) {
            if (sessions.none { it.name == name }) {
                val n = 20 + sessions.size
                // Made detached at tmux's default size; its window follows the device.
                val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", repo, false, false)
                followers += pane.id
                val repoName = repo.split("/").lastOrNull { it.isNotEmpty() }
                sessions = sessions + Session("$$n", name, repoName, repo, branch, panes = listOf(pane))
            }
        }
        emit(Message.TaskCreated(id, name))
        ok(id)
        emit(Message.SessionsChanged)
    }

    /**
     * Records a Task and starts a session for it, with a running agent. [FAILING_BRANCH]
     * keeps the record with a note and no session, as a failed launch does on a host.
     */
    private fun createTask(req: Request.CreateTask) {
        if (req.worker != null) {
            fail(req.id, ErrorCode.UNSUPPORTED, "Workers are not implemented in the demo.")
            return
        }
        val workspace = content.workspaces.firstOrNull { it.name == req.workspace }
        if (workspace == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.workspace} is not a workspace")
            return
        }
        req.loop?.let { spec -> loopRefusal(workspace, spec) }?.let {
            fail(req.id, ErrorCode.BAD_REQUEST, it)
            return
        }
        // A level 2 host names the branch after the title.
        val branch = req.branch ?: req.repo?.let { if (workLevel >= WORK_ASK) slug(req.title) else null }
        if (req.repo != null && branch == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "a task with a repo needs a branch")
            return
        }
        val now = now()
        val failed = branch == FAILING_BRANCH
        // One Task per Work item `rev` (ADR 0048); an older host ignores `work`.
        val work = req.work?.takeIf { workLevel >= WORK_ASSISTANT }
        val made = try {
            synchronized(lock) { work?.let { assistant.taskFor(it, req.workRev ?: -1) }?.let { id -> tasks.first { it.id == id } } }
        } catch (e: DemoWork.Refusal) {
            return fail(req.id, e.code, e.message ?: "")
        }
        if (made != null) {
            emit(Message.TaskSaved(req.id, made))
            return ok(req.id)
        }
        val record = synchronized(lock) {
            val id = "t-new%03d".format(++createdCount)
            val name = slug(req.title)
            val dir = req.repo?.let { "/src/.worktrees/$name" } ?: "/${req.workspace}/tasks/$name-$id"
            val record = TaskRecord(
                id, rev = 1, workspace = req.workspace, title = req.title, goal = req.goal, status = TaskStatus.OPEN,
                createdAt = now, updatedAt = now, project = req.project, repo = req.repo, branch = branch,
                dir = dir, session = name.takeUnless { failed }, loop = req.loop?.let { startedLoop(null, it, reset = true, now) },
                note = if (failed) "could not create a worktree for $FAILING_BRANCH: a branch named '$FAILING_BRANCH' already exists" else null,
                work = work,
            )
            tasks = tasks + record
            if (work != null) assistant.started(work, req.workRev ?: -1, id, now)
            if (!failed) {
                val n = 20 + sessions.size
                val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", dir, false, false, Agent(AgentState.RUNNING, now))
                followers += pane.id
                val repoName = req.repo?.split("/")?.lastOrNull { it.isNotEmpty() }
                sessions = sessions + Session("$$n", name, repoName, req.repo, branch, taskId = id, panes = listOf(pane))
            }
            record
        }
        emit(Message.TaskSaved(req.id, record))
        if (failed) {
            fail(req.id, ErrorCode.TASK_FAILED, record.note ?: "")
            return
        }
        ok(req.id)
        emit(Message.SessionsChanged)
    }

    /**
     * An ask session: the workspace's agent in the repo's main checkout, with no Task and no
     * record (ADR 0031). Its name counts the ask sessions of that repo.
     */
    private fun startSession(req: Request.StartSession) {
        if (workLevel < WORK_ASK) return fail(req.id, ErrorCode.UNSUPPORTED, NO_REQUEST)
        val workspace = content.workspaces.firstOrNull { it.name == req.workspace }
            ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.workspace} is not a workspace")
        val dir = if (req.repo.startsWith("/")) req.repo else workspace.root?.let { "${it.trimEnd('/')}/${req.repo}" }
        if (dir == null || dir !in workspace.repos) {
            return fail(req.id, ErrorCode.BAD_REQUEST, "${req.repo} is not a repo of ${req.workspace}")
        }
        val repoName = dir.substringAfterLast('/')
        val started = synchronized(lock) {
            val n = 20 + sessions.size
            val name = "ask-$repoName-${1 + sessions.count { it.name.startsWith("ask-$repoName-") }}"
            val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", dir, false, false, Agent(AgentState.RUNNING, now()))
            followers += pane.id
            sessions = sessions + Session("$$n", name, repoName, dir, "main", ask = workspace.name, panes = listOf(pane))
            Message.SessionStarted(req.id, "$$n", name)
        }
        emit(started)
        ok(req.id)
        emit(Message.SessionsChanged)
    }

    /** Sets a status, refusing a stale [Request.UpdateTask.rev] as the bridge does. */
    private fun updateTask(req: Request.UpdateTask) {
        val status = req.status
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == req.task } ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.task} is not a task")
            if (current.rev != req.rev) return fail(req.id, ErrorCode.CONFLICT, "the task changed since you read it")
            if (status in ENDS) lostWork(current.id, req.worktree)?.let { return fail(req.id, ErrorCode.WOULD_LOSE_WORK, it) }
            val now = now()
            val saved = current.copy(rev = current.rev + 1, status = status, updatedAt = now, loop = current.loop?.afterStatus(status, now))
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            saved
        }
        emit(Message.TaskSaved(req.id, saved))
        ok(req.id)
    }

    /**
     * Why removing [task]'s worktree could lose work, when [worktree] asks to remove it and does not force it.
     * The demo removes nothing: a forced end only saves the status.
     */
    private fun lostWork(task: String?, worktree: WorktreeEnd?): String? =
        UNMERGED.takeIf { worktree == WorktreeEnd.REMOVE && task in content.unmerged }

    /** Starts a Task's loop, also a stopped or passed one, when its workspace allows the check. */
    private fun startLoop(req: Request.StartLoop) = changeLoop(req.id, req.task) { current, now ->
        val refusal = loopRefusal(content.workspaces.firstOrNull { it.name == current.workspace }, req.loop)
        if (refusal != null) LoopChange.Refused(refusal) else LoopChange.Saved(startedLoop(current.loop, req.loop, req.reset, now))
    }

    private fun stopLoop(req: Request.StopLoop) = changeLoop(req.id, req.task) { current, now ->
        current.loop?.let { LoopChange.Saved(it.stopped(now)) } ?: LoopChange.Refused("${req.task} has no loop")
    }

    private sealed interface LoopChange {
        data class Saved(val loop: TaskLoop) : LoopChange
        data class Refused(val reason: String) : LoopChange
    }

    /**
     * Saves the loop [change] makes, with a new rev, then `task-saved`, `result`, and
     * `tasks-changed` as the host's poll would send it to every bridge.
     */
    private fun changeLoop(id: Long, task: String, change: (TaskRecord, Long) -> LoopChange) {
        val now = now()
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == task } ?: return fail(id, ErrorCode.BAD_REQUEST, "$task is not a task")
            val loop = when (val result = change(current, now)) {
                is LoopChange.Refused -> return fail(id, ErrorCode.BAD_REQUEST, result.reason)
                is LoopChange.Saved -> result.loop
            }
            val saved = current.copy(rev = current.rev + 1, updatedAt = now, loop = loop)
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            saved
        }
        emit(Message.TaskSaved(id, saved))
        ok(id)
        emit(Message.TasksChanged)
    }

    /**
     * Restarts a Task's agent as the bridge does (ADR 0018): ends the open run, adds a
     * restart run, and the agent runs again. A running agent needs [Request.RestartAgent.force].
     */
    private fun restartAgent(req: Request.RestartAgent) {
        if (req.worker != null || req.rev != null) {
            fail(req.id, ErrorCode.UNSUPPORTED, "Workers are not implemented in the demo.")
            return
        }
        val now = now()
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == req.task } ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.task} is not a task")
            val session = sessions.firstOrNull { it.taskId == current.id }
            val pane = session?.panes?.firstOrNull { it.role == Role.AGENT }
                ?: return fail(req.id, ErrorCode.TASK_FAILED, "No session runs this task's agent.")
            if (pane.agent?.state == AgentState.RUNNING && !req.force) {
                return fail(req.id, ErrorCode.BAD_REQUEST, "The agent is running. Restart with force to stop it.")
            }
            val ended = current.runs.map { if (it.endedAt == null) it.copy(endedAt = now) else it }
            val run = AgentRun(
                id = (current.runs.maxOfOrNull { it.id } ?: 0) + 1, agent = "claude-code", startedAt = now,
                agentSession = UUID.randomUUID().toString(), reason = RunReason.RESTART,
            )
            val saved = current.copy(rev = current.rev + 1, updatedAt = now, note = null, runs = ended + run)
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            val running = pane.copy(agent = Agent(AgentState.RUNNING, now))
            sessions = sessions.map { s ->
                if (s.id != session.id) s else s.copy(panes = s.panes.map { if (it.id == pane.id) running else it })
            }
            saved
        }
        emit(Message.TaskSaved(req.id, saved))
        ok(req.id)
        emit(Message.SessionsChanged)
    }

    private fun agentWaits(session: String) {
        synchronized(lock) {
            sessions = sessions.map { s ->
                if (s.id != session || s.panes.isEmpty()) return@map s
                val first = s.panes[0].copy(agent = Agent(AgentState.WAITING, now()))
                s.copy(panes = listOf(first) + s.panes.drop(1))
            }
        }
        emit(Message.SessionsChanged)
    }

    /** fix-login is not merged, so only keep and force close it. An ask session closes too (ADR 0039). */
    private fun closeTask(id: Long, session: String, mode: CloseMode) {
        val found = synchronized(lock) { sessions.firstOrNull { it.id == session } }
        val isAsk = workLevel >= WORK_CLOSE_ASK && found?.ask != null
        if (found == null || found.worktree == null && !isAsk) {
            fail(id, ErrorCode.BAD_REQUEST, "$session is not a task")
            return
        }
        if (found.name == "fix-login" && mode == CloseMode.CLOSE) {
            fail(
                id, ErrorCode.TASK_FAILED,
                "fix-login is not merged. Close it and keep the worktree, or remove the worktree anyway.",
            )
            return
        }
        synchronized(lock) { sessions = sessions.filter { it.id != session } }
        ok(id)
        emit(Message.SessionsChanged)
    }

    private fun ok(id: Long) = emit(Message.Result(id))

    private fun fail(id: Long, code: ErrorCode, message: String) = emit(Message.Result(id, ErrorBody(code, message)))

    /**
     * The demo agent: echoes a prompt. `/wait` makes it wait on a menu it cannot
     * name, and `/allow`, `/ask` and `/plan` on a permission, a question and a plan.
     */
    private fun answer(text: String) {
        val n = synchronized(lock) {
            chat?.let { it.count += 1 }
            chat?.count ?: 0
        }
        val user = ChatEntry("u:demo$n", ChatEntry.Kind.USER, text)
        val waiting = demoPrompt(text, n)
        when {
            text == "/wait" -> emitChat(listOf(user), AgentState.WAITING)
            waiting != null -> {
                synchronized(lock) { waitingOn = waiting }
                emitChat(listOf(user, waiting.second), AgentState.WAITING, prompt = waiting.first)
            }
            else -> {
                val reply = ChatEntry("a:demo$n", ChatEntry.Kind.ASSISTANT, "You said: **$text**")
                emitChat(listOf(user, reply), AgentState.IDLE)
            }
        }
    }

    /** Keys that pick an option of the waiting prompt answer it. */
    private fun pick(keys: List<String>) {
        val (prompt, tool) = synchronized(lock) { waitingOn } ?: return
        val option = prompt.options.firstOrNull { it.keys == keys } ?: return
        synchronized(lock) { waitingOn = null }
        val result = if (option.cancel) "Declined" else "Chose ${option.label}"
        val done = tool.copy(tool = tool.tool?.copy(result = result, error = option.cancel))
        emitChat(listOf(done), AgentState.IDLE)
    }

    private fun emitChat(
        entries: List<ChatEntry>,
        status: AgentState,
        reset: Boolean = false,
        prompt: ChatPrompt? = null,
    ) {
        val (id, seq) = synchronized(lock) {
            val c = chat ?: return
            c.seq += 1
            c.id to c.seq
        }
        emit(
            Message.ChatEntries(
                chat = id, seq = seq, reset = reset, entries = entries, status = Agent(status, now()),
                agent = if (reset) "claude-code" else null, prompt = prompt,
            ),
        )
    }

    private fun echo(pane: String, data: ByteArray) {
        val w = synchronized(lock) { watch }
        emit(Message.Output(w, pane, Bytes(data)))
    }

    private fun emit(m: Message) {
        out.trySend(Wire.encode(m))
    }

    companion object {
        const val CHAT_PANE = "%1"

        /** The Inbox answers whose text goes to the agent as its next prompt. */
        private val PROMPT_ACTIONS = setOf(InboxAction.REPLY)
        private val PROTECT_ACTIONS = setOf(InboxAction.PROTECT, InboxAction.UNPROTECT)

        /** The Work item actions a level 11 host records as corrections (ADR 0050 §2). */
        private val WORK_CORRECTIONS = mapOf(
            WorkAction.LATER to CorrectionKind.DEFERRED,
            WorkAction.PIN to CorrectionKind.PINNED,
            WorkAction.UNPIN to CorrectionKind.UNPINNED,
        )
        private const val MAX_DELETE_DAYS = 3650
        private const val DAY_SECONDS = 86_400L
        private const val OLD_ITEM_AGE = 4 * DAY_SECONDS

        /** Where the bridge writes a Task's brief for its agent (ADR 0015). */
        const val TASK_DOC = ".muxa/TASK.md"

        /** Where the bridge writes the facts for a restarted agent (ADR 0018). */
        const val HANDOFF_DOC = ".muxa/HANDOFF.md"

        /** Where the daemon writes the last check's result (ADR 0019). */
        const val CHECK_DOC = ".muxa/CHECK.md"

        /** A branch whose task fails, to show the error. */
        const val FAILING_BRANCH = "taken"

        private val CAPABILITIES =
            listOf(
                "chat", "docs", "tasks", "changes", "close-task", "images", "branch-changes", "task-records", "task-handoff",
                "task-loop", "tasks-changed", "viewport", "history",
            )

        /** The statuses that end a Task, and so take `worktree` (ADR 0043). */
        private val ENDS = setOf(TaskStatus.DONE, TaskStatus.DROPPED)
        private const val UNMERGED = "its branch is not merged"

        /** Lines of earlier output a demo pane has. */
        private const val HISTORY_LINES = 60

        /** What a bridge says to a request it does not know. */
        const val NO_REQUEST = "The demo bridge does not support this request."

        /** A little after a run should have moved on, the app lists again. */
        private const val RUN_TICK_MILLIS = 100L
        private const val UPDATE_DELAY_MILLIS = 2_000L

        private fun now() = System.currentTimeMillis() / 1_000

        /** A session name from a title: lowercase letters, digits and dashes, at most 20 characters. */
        internal fun slug(title: String): String =
            title.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-').take(20).trim('-').ifEmpty { "task" }

        /** What the bridge would write into `TASK.md`. */
        internal fun brief(task: TaskRecord) = buildString {
            append("# ").append(task.title).append("\n\n")
            append("## Goal\n\n").append(task.goal).append("\n\n")
            append("## Workspace\n\n").append(task.workspace)
            task.project?.let { append(" · ").append(it) }
            task.repo?.let { append("\n\n## Resources\n\n- Repo: `").append(it).append("`, branch `").append(task.branch ?: "").append("`") }
            append("\n")
        }

        /** When the bridge last wrote HANDOFF.md: the newest restart, else when the Task began. */
        internal fun handoffTime(task: TaskRecord): Long =
            task.runs.filter { it.reason == RunReason.RESTART }.maxOfOrNull { it.startedAt } ?: task.createdAt

        /** What the bridge would write into `HANDOFF.md`, with sample git facts. The iOS demo writes the same. */
        internal fun handoff(task: TaskRecord) = buildString {
            append("# Handoff\n\n## Task\n\n")
            append("- Status: ").append(task.status.raw).append("\n")
            append("- Note: ").append(task.note ?: "none").append("\n\n## Runs\n\n")
            for (run in task.runs) {
                append("- Run ").append(run.id).append(" · ").append(run.agent ?: "unknown agent")
                append(" · ").append(run.agentSession ?: "no session id").append("\n")
            }
            task.branch?.let { append("\n## Git\n\n- Branch: ").append(it).append("\n- 3 commits since origin/main\n") }
        }

        internal fun demoPrompt(text: String, n: Int): Pair<ChatPrompt, ChatEntry>? {
            val entry = "t:demo$n"
            fun tool(name: String, summary: String) = ChatEntry(entry, ChatEntry.Kind.TOOL, tool = ChatTool(name, summary))
            return when (text) {
                "/allow" -> ChatPrompt(
                    entry, ChatPrompt.Kind.PERMISSION,
                    options = listOf(
                        ChatPrompt.Option("Allow", listOf("1")),
                        ChatPrompt.Option("Deny", listOf("Escape"), cancel = true),
                    ),
                ) to tool("Bash", "rm -rf build")
                "/ask" -> ChatPrompt(
                    entry, ChatPrompt.Kind.QUESTION, "Which format should the config use?",
                    listOf(ChatPrompt.Option("TOML", listOf("1")), ChatPrompt.Option("JSON", listOf("2"))),
                ) to tool("AskUserQuestion", "Which format should the config use?")
                "/plan" -> ChatPrompt(
                    entry, ChatPrompt.Kind.PLAN,
                    options = listOf(ChatPrompt.Option("Keep planning", listOf("Escape"), cancel = true)),
                ) to tool("ExitPlanMode", "Reader plan")
                else -> null
            }
        }

    }
}
