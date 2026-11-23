import Foundation

/// One Bot's work, grouped by the rules of PROTOCOL-v2.md, "A Bot's work"
/// (ADR 0086), from what a client already has: the Inbox, the live terminals
/// and the Routines. Kotlin's `:v2` follows the same rules.
public struct BotWork: Equatable, Sendable {
    public enum Status: String, Sendable {
        case needsYou = "needs-you", running, paused, idle
    }

    /// A row of Needs you: an item first, then a waiting terminal.
    public enum Entry: Equatable, Sendable, Identifiable {
        case item(InboxItem)
        case terminal(Terminal)

        public var id: String {
            switch self {
            case let .item(i): i.id
            case let .terminal(t): t.id
            }
        }
    }

    /// Its open items, Later ones only once they are back.
    public var items: [InboxItem]
    /// Its live terminals: talks and runs (`muxa.bot`), and its members' runs (`muxa.lead`).
    public var terminals: [Terminal]
    public var needsYou: [Entry]
    public var inProgress: [Terminal]
    /// Enabled Routines with a next run, soonest first, then those on review.
    public var planned: [Routine]
    /// Its done items, newest first.
    public var recent: [InboxItem]
    public var status: Status

    /// `enabled`: the Bot's own switch in `BotSpec`. `now` is Unix seconds.
    public init(bot: String, workspace: String, enabled: Bool, inbox: Inbox, terminals all: [Terminal],
                routines: [Routine], now: Int64) {
        func mine(_ i: InboxItem) -> Bool { i.bot == bot && i.workspace == workspace }
        items = inbox.items.filter { mine($0) && !$0.isLater(now: now) }
        terminals = all.filter {
            $0.live && $0.workspace == workspace && ($0.bot == bot || $0.lead == bot)
        }
        let waiting = terminals.filter { ["blocked", "waiting"].contains($0.status?.state ?? "") }
        needsYou = items.map(Entry.item) + waiting.map(Entry.terminal)
        inProgress = terminals.filter { t in !waiting.contains { $0.id == t.id } }
        let own = routines.filter { $0.bot == bot && $0.workspace == workspace && $0.enabled }
        planned = own.filter { $0.nextDue != nil }.sorted { ($0.nextDue ?? 0) < ($1.nextDue ?? 0) }
            + own.filter { $0.nextDue == nil && $0.on == "review" }
        recent = inbox.done.filter(mine)
        status = !needsYou.isEmpty ? .needsYou : !terminals.isEmpty ? .running : !enabled ? .paused : .idle
    }
}

extension Terminal {
    /// The rules' terminal label: the Workspace it runs in.
    fileprivate var workspace: String? { labels["muxa.workspace"] }
}

extension SettingsWorkspace {
    /// Removing Bot `name`: each of its Routines moves to `target` (nil: no
    /// Bot) by a full `routine-update`, then `bot-remove`. Sent as one
    /// `settings-update`, so all of it applies or none.
    public func botRemoval(_ name: String, movingRoutinesTo target: String?) -> [SettingsOp] {
        let moves = routines.filter { $0.spec.bot == name }.map { r -> SettingsOp in
            var spec = r.spec
            spec.bot = target
            return .routineUpdate(workspace: self.name, routine: spec)
        }
        return moves + [.botRemove(workspace: self.name, name: name)]
    }
}
