import MuxaProtocol
import SwiftUI

/// The app's colors by meaning. Views use these names only; the light and dark
/// values live in Assets.xcassets. Besides the accent, only states have a color.
enum Theme {
    static let accent = Color.accentColor
    static let chatBackground = Color("ChatBackground")
    static let userBubble = Color("UserBubble")
    static let toolCard = Color("ToolCard")
    static let composer = Color("Composer")
    /// Behind the pane: the terminal's own background.
    static let terminal = Color.black

    static let connected = Color.green
    static let attention = Color.orange
    static let failure = Color.red
    /// ✓ closes an item: a green dark enough for an icon on white (ADR 0040).
    static let done = Color("Done")
    /// Later lowers an item's priority: the neutral gray of less important things.
    static let later = Color.secondary
    /// Lines a change adds and removes.
    static let added = Color.green
    static let removed = Color.red

    static func state(_ s: AgentState) -> Color {
        switch s {
        case .running: .blue
        case .waiting: attention
        case .idle: .gray
        case .done: .green
        case .error: failure
        }
    }
}

/// One SF Symbol per concept, used everywhere that concept appears.
enum Icon {
    static let host = "desktopcomputer"
    static let session = "rectangle.stack"
    static let repo = "folder"
    static let document = "doc.text"
    static let plan = "list.bullet.clipboard"
    static let image = "photo"
    static let chat = "text.bubble"
    static let agent = "terminal"
    static let shell = "apple.terminal"
    static let fit = "textformat.size"
    static let more = "ellipsis.circle"
    static let history = "arrow.up.to.line"
    static let disconnect = "xmark.circle"
    static let add = "plus"
    static let newTask = "plus.square.on.square"
    static let closeTask = "xmark.square"
    static let tasks = "checklist"
    static let changes = "plusminus"
    static let edit = "pencil"
    static let delete = "trash"
    static let demo = "play.circle"
    static let leaveDemo = "rectangle.portrait.and.arrow.right"
    static let warning = "exclamationmark.triangle"
    static let waiting = "hand.raised.fill"
    static let send = "arrow.up.circle.fill"
    static let routine = "clock.arrow.circlepath"
    static let briefing = "newspaper"
    static let proposal = "lightbulb"
    /// Music's "Play Last": to the end of the queue (ADR 0040).
    static let later = "text.line.last.and.arrowtriangle.forward"
    static let done = "checkmark"
    static let openSession = "arrow.up.right.square"
    static let settings = "gearshape"
    static let ask = "questionmark.bubble"
    /// `+` at `work` level 9: a thought saved on the host (ADR 0048).
    static let capture = "square.and.pencil"
    static let work = "list.bullet.rectangle"
    static let note = "note.text"
    static let reopen = "arrow.uturn.backward"
    static let withdraw = "xmark.circle"
    /// A workspace's assistant working on a Work item (ADR 0048).
    static let assistant = "sparkles"
    static let cancel = "stop.circle"
    static let result = "doc.text.magnifyingglass"
    /// The overall assistant's plan, and the user's pins and due dates (ADR 0049).
    static let today = "sun.max"
    static let pin = "pin"
    static let unpin = "pin.slash"
    static let due = "calendar.badge.clock"
    /// A protected Inbox item, which Delete Old keeps (ADR 0046).
    static let protected = "lock.fill"
    static let protect = "lock"
    static let unprotect = "lock.open"
    /// What the user told the overall assistant (ADR 0050).
    static let policy = "text.book.closed"
    static let notToday = "moon.zzz"
    static let forget = "eraser"
    static let moveUp = "arrow.up"
    static let moveDown = "arrow.down"
}
