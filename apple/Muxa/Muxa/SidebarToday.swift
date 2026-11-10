import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Inbox's Today section (ADR 0049 §5): the newest plan's rows, each with its
/// reason, its minutes and a Why disclosure, then the Due soon line and Plan Now. A new
/// plan replaces the rows and shows New until a row is opened; it never changes the
/// selection, which only a tap on a row does. At `work` level 11 the user drags rows
/// into another order and removes one with Not today (ADR 0050 §7); a removed row
/// collapses to one line.
struct SidebarTodaySection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let plan = model.session.currentPlan
        let newest = model.session.newestPlan
        Section {
            if let newest, newest.id != plan?.id {
                PlanStatusRow(plan: newest)
            }
            if let plan {
                rows(plan)
                if plan.top.isEmpty {
                    Text("Nothing to rank right now.").foregroundStyle(.secondary)
                        .accessibilityIdentifier("today-empty")
                }
                dueSoon(plan)
            } else if newest == nil {
                Text("No plan yet.").foregroundStyle(.secondary)
                    .accessibilityIdentifier("today-empty")
            }
            if model.session.assistantInfo?.overall != nil {
                PlanNowRow()
            }
            if model.usesPolicies {
                Button("Policies", systemImage: Icon.policy) { model.showPolicies = true }
                    .accessibilityIdentifier("today-policies")
            }
        } header: {
            // One element: a list's header does not keep a control of its own. A row's tap clears New.
            HStack(spacing: 6) {
                Text("Today")
                if model.hasNewPlan {
                    Text("New").font(.caption.weight(.semibold)).foregroundStyle(Theme.accent)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("inbox-section-today")
        }
        .task(id: plan?.id) { model.notePlanShown() }
    }

    /// The rows in the user's order, if any. Only a level 11 host takes a new order.
    @ViewBuilder
    private func rows(_ plan: Plan) -> some View {
        let edits = model.usesPolicies
        let rows = edits ? model.session.todayRows(of: plan) : plan.top
        ForEach(Array(rows.enumerated()), id: \.element.target) { index, row in
            if edits, model.session.isDismissed(row, in: plan) {
                DismissedTodayRow(row: row)
            } else if edits, let id = row.rowId {
                TodayRow(row: row)
                    .contextMenu {
                        if index > 0 {
                            Button("Move Up", systemImage: Icon.moveUp) { model.moveToday(plan, row: id, by: -1) }
                        }
                        if index < rows.count - 1 {
                            Button("Move Down", systemImage: Icon.moveDown) { model.moveToday(plan, row: id, by: 1) }
                        }
                        Button("Not Today…", systemImage: Icon.notToday) { model.notTodayRow = row }
                    }
                    .swipeActions(edge: .trailing) {
                        Button("Not Today", systemImage: Icon.notToday) { model.notTodayRow = row }.tint(.gray)
                    }
            } else {
                TodayRow(row: row)
            }
        }
        .onMove(perform: edits ? { model.moveToday(plan, from: $0, to: $1) } : nil)
    }

    /// Work items moved down with Later that are due soon: one line, never rows.
    @ViewBuilder
    private func dueSoon(_ plan: Plan) -> some View {
        if !plan.dueSoon.isEmpty {
            let titles = plan.dueSoon.map { model.workItem($0)?.title ?? $0 }
            Label("Due soon, moved down by you: \(titles.joined(separator: ", "))", systemImage: Icon.due)
                .font(.caption)
                .foregroundStyle(Theme.attention)
                .accessibilityIdentifier("today-due-soon")
        }
    }
}

/// One row of the plan: the target's title, why, the minutes and what comes next. The
/// title opens the Work item or the Inbox item; Why shows the basis and what is missing.
struct TodayRow: View {
    @Environment(AppModel.self) private var model
    let row: PlanRow
    @State private var showsWhy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button { model.open(row) } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.title(of: row)).lineLimit(2)
                    Text(row.why).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    Text("\(row.minutes.shown) · Next: \(row.next)").font(.caption).foregroundStyle(.secondary)
                        .lineLimit(2)
                    if let mark = row.work.flatMap(model.workItem).flatMap(model.requestMark) {
                        Text(mark).font(.caption).foregroundStyle(Theme.attention)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .accessibilityIdentifier("today-row-\(row.target)")
            if !row.basis.isEmpty || !row.missing.isEmpty {
                Button(showsWhy ? "Hide Why" : "Why") { showsWhy.toggle() }
                    .font(.caption)
                    .buttonStyle(.borderless)
                    .accessibilityIdentifier("today-why-\(row.target)")
                if showsWhy { why }
            }
        }
    }

    private var why: some View {
        VStack(alignment: .leading, spacing: 4) {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(row.basis.enumerated()), id: \.offset) { _, basis in
                    VStack(alignment: .leading, spacing: 1) {
                        if let cited = model.session.cited(basis) {
                            Text("\(basis.kind.shown): \(model.shown(cited))").font(.caption)
                        } else {
                            Text("\(basis.kind.shown): \(basis.text)").font(.caption)
                            let detail = [basis.at?.shownDate, basis.source].compactMap { $0 }
                            if !detail.isEmpty {
                                Text(detail.joined(separator: " · ")).font(.caption2).foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                            }
                        }
                    }
                }
                ForEach(Array(row.missing.enumerated()), id: \.offset) { _, missing in
                    Text("Missing: \(missing)").font(.caption).foregroundStyle(.secondary)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("today-basis-\(row.target)")
            CitedRecordActions(basis: row.basis)
        }
    }
}

/// Edit on a cited policy, Forget on a cited hypothesis or correction, with what Forget
/// leaves (ADR 0050 §5).
private struct CitedRecordActions: View {
    @Environment(AppModel.self) private var model
    let basis: [Basis]
    @State private var editing: EditedPolicy?
    @State private var forgetting: String?
    @State private var error: String?

    var body: some View {
        let cited = basis.compactMap(model.session.cited)
        let forgettable = cited.filter(\.isForgettable)
        if !cited.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(cited.enumerated()), id: \.offset) { _, record in
                    switch record {
                    case .policy(let policy):
                        Button("Edit Policy: \(policy.text)") { editing = EditedPolicy(policy: policy) }
                            .lineLimit(1)
                            .accessibilityIdentifier("today-edit-\(policy.id)")
                    case .hypothesis(let h):
                        Button("Forget: \(h.text)") { forgetting = h.id }
                            .lineLimit(1)
                            .accessibilityIdentifier("today-forget-\(h.id)")
                    case .correction(let c):
                        Button("Forget: \(model.shown(c))") { forgetting = c.id }
                            .lineLimit(1)
                            .accessibilityIdentifier("today-forget-\(c.id)")
                    case .forgotten:
                        EmptyView()
                    }
                }
                if !forgettable.isEmpty {
                    Text(PaneSession.forgetNote).font(.caption2).foregroundStyle(.secondary)
                }
                if let error {
                    Label(error, systemImage: Icon.warning).font(.caption).foregroundStyle(Theme.failure)
                }
            }
            .font(.caption)
            .buttonStyle(.borderless)
            .sheet(item: $editing) { edited in PolicyEditor(policy: edited.policy) }
            .forgetConfirmation(record: $forgetting) { record in
                Task { error = await model.forget(record) }
            }
        }
    }
}

/// A row the user removed from Today: one line, still opening its item.
private struct DismissedTodayRow: View {
    @Environment(AppModel.self) private var model
    let row: PlanRow

    var body: some View {
        Button { model.open(row) } label: {
            Label("Not today: \(model.title(of: row))", systemImage: Icon.notToday)
                .font(.caption)
                .lineLimit(1)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .accessibilityIdentifier("today-dismissed-\(row.target)")
    }
}

/// The newest plan when it has no rows: being made, or why it failed.
private struct PlanStatusRow: View {
    let plan: Plan

    var body: some View {
        if plan.delivery.isActive {
            HStack(spacing: 8) {
                ProgressView()
                Text("Making a plan…").foregroundStyle(.secondary)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("today-planning")
        } else if plan.delivery != .resulted {
            // The host's own text, naming ids and limits (PROTOCOL.md): shown as it is.
            Label(plan.reason ?? "The plan \(plan.delivery.shown).", systemImage: Icon.warning)
                .font(.callout)
                .foregroundStyle(Theme.attention)
                .accessibilityIdentifier("today-plan-reason")
        }
    }
}

/// Plan Now asks the overall assistant for a plan; a long press also says the time you have.
private struct PlanNowRow: View {
    @Environment(AppModel.self) private var model
    @State private var asking = false
    @State private var error: String?

    /// The time the user has, in minutes, as the menu offers it.
    private static let choices: [(UInt32, String)] = [(30, "30 minutes"), (60, "1 hour"), (120, "2 hours")]

    var body: some View {
        // A plain button: a list row's Menu with a primary action opens the menu on a tap.
        Button("Plan Now", systemImage: Icon.today) { plan(nil) }
            .contextMenu {
                ForEach(Self.choices, id: \.0) { minutes, label in
                    Button("I Have \(label)") { plan(minutes) }
                }
            }
        .disabled(asking || model.session.isPlanning || !model.isConnected)
        .accessibilityIdentifier("today-plan-now")
        if let error {
            Label(error, systemImage: Icon.warning).font(.callout).foregroundStyle(Theme.failure)
                .accessibilityIdentifier("today-plan-error")
        }
    }

    private func plan(_ minutes: UInt32?) {
        asking = true
        error = nil
        Task {
            defer { asking = false }
            error = await model.planNow(minutes: minutes)
        }
    }
}

extension PlanRow {
    /// The Work item or the Inbox item the row names: unique within a plan.
    var target: String { work ?? item ?? why }
}

/// Not today's optional reason, on the sidebar's list: a row's own alert would go with
/// the row when it scrolls away.
struct NotTodayPrompt: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        @Bindable var model = model
        content.modifier(ReasonPrompt(
            title: "Not Today", button: "Not Today", subject: $model.notTodayRow, identifier: "not-today",
            message: "Why, if you want to say. The assistant reads it next time.") { row, reason in
                guard let id = row.rowId else { return }
                model.notToday(id, reason: reason)
            })
    }
}
