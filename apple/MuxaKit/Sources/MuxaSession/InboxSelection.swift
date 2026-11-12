/// Which item the Inbox selects after its list changed. The same rule as muxa for tmux
/// and Android: the selection stays on its item, or moves to the next when the item left.
public enum InboxSelection {
    /// `movedDown` is Later (ADR 0040): the item stays listed at the bottom, but counts as left.
    public static func next(current: String, before: [String], after: [String], movedDown: Bool) -> String? {
        if !movedDown, after.contains(current) { return current }
        let below = before.firstIndex(of: current).map { before.dropFirst($0 + 1) } ?? before[...]
        if let next = below.first(where: { $0 != current && after.contains($0) }) { return next }
        if let last = after.last(where: { $0 != current }) { return last }
        return after.contains(current) ? current : nil
    }
}
