import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Inbox's Today section (ADR 0047 §5): the newest plan's rows, each with its
/// reason, its minutes and a Why disclosure, then the Due soon line and Plan Now. A new
/// plan replaces the rows and shows New until a row is opened; it never changes the
/// selection, which only a tap on a row does.
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
                ForEach(plan.top, id: \.target) { row in
                    TodayRow(row: row)
                }
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
            ForEach(Array(row.basis.enumerated()), id: \.offset) { _, basis in
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(basis.kind.shown): \(basis.text)").font(.caption)
                    let detail = [basis.at?.shownDate, basis.source].compactMap { $0 }
                    if !detail.isEmpty {
                        Text(detail.joined(separator: " · ")).font(.caption2).foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                }
            }
            ForEach(Array(row.missing.enumerated()), id: \.offset) { _, missing in
                Text("Missing: \(missing)").font(.caption).foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("today-basis-\(row.target)")
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
