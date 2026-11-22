import MuxaV2
import SwiftUI

/// The app's colors by meaning. The values are the design mocks' dark palette;
/// the app forces dark appearance to match them.
enum Theme {
    static let accent = Palette.accent
    static let chatBackground = Palette.ground
    static let userBubble = Palette.raised
    static let toolCard = Palette.card
    static let composer = Palette.surface
    /// Behind the pane: the terminal's own background.
    static let terminal = Palette.terminal
    static let terminalText = Palette.terminalText

    static let connected = TaskState.working.color
    static let attention = TaskState.needsYou.color
    static let failure = Palette.late
    /// ✓ closes an item (ADR 0040).
    static let done = Palette.swipeDone
    /// Later lowers an item's priority.
    static let later = Palette.swipeLater
    /// Lines a change adds and removes.
    static let added = TaskState.working.color
    static let removed = Palette.late
}

/// The design mocks' palette. Views reach it through `Theme`, the kind and
/// state colors, and the shared chrome in `Next/Chrome.swift`.
enum Palette {
    static let ground = Color(hex: 0x0E1014)
    static let surface = Color(hex: 0x14171C)
    static let card = Color(hex: 0x161A20)
    /// Icon buttons and other raised controls.
    static let raised = Color(hex: 0x1E232B)
    /// The picked segment.
    static let selected = Color(hex: 0x2A2F38)
    /// The picked row of a list.
    static let rowSelected = Color(hex: 0x232831)
    static let border = Color(hex: 0x262B33)
    static let borderStrong = Color(hex: 0x2C323C)
    /// A line between rows inside a card.
    static let divider = Color(hex: 0x20252D)
    static let text = Color(hex: 0xE7E9EC)
    static let textSecondary = Color(hex: 0xA3ABB6)
    static let textTertiary = Color(hex: 0x8B94A0)
    static let accent = Color(hex: 0x6AA6FF)
    static let terminal = Color(hex: 0x0A0C0F)
    static let terminalText = Color(hex: 0xD5D9DF)
    /// A due date that has passed, and errors.
    static let late = Color(hex: 0xFF9B9B)
    /// The Needs-you banner on Home.
    static let bannerBack = Color(hex: 0x1E1A12)
    static let bannerBorder = Color(hex: 0x4A3A18)
    /// The bar above a waiting Task.
    static let promptBack = Color(hex: 0x2A2110)
    static let promptBorder = Color(hex: 0x5A4416)
    /// Text on an amber button.
    static let onAmber = Color(hex: 0x1A1203)
    static let swipeLater = Color(hex: 0x5B4BB8)
    static let swipeDone = Color(hex: 0x2E8B6A)
}

extension Color {
    /// `0xRRGGBB` in sRGB.
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255)
    }
}

/// What a Task's agent reports about itself (OSC 7501), as the mocks color it.
/// No report is idle: muxa invents no state.
enum TaskState: CaseIterable {
    case needsYou, done, working, idle

    init(_ state: String?) {
        switch state {
        case "blocked", "waiting": self = .needsYou
        case "done": self = .done
        case "working": self = .working
        default: self = .idle
        }
    }

    var title: String {
        switch self {
        case .needsYou: "Needs you"
        case .done: "Done"
        case .working: "Working"
        case .idle: "Idle"
        }
    }

    var color: Color {
        switch self {
        case .needsYou: Color(hex: 0xF5A524)
        case .done: Color(hex: 0x6AA6FF)
        case .working: Color(hex: 0x3FC79A)
        case .idle: Color(hex: 0x8B94A0)
        }
    }
}

extension ItemKind {
    /// The chip's text and background, as PhoneInbox's `K`. To do is neutral.
    var chipColors: (text: Color, back: Color) {
        switch self {
        case .review: (Color(hex: 0x7FE0B8), Color(hex: 0x123326))
        case .failed: (Color(hex: 0xFF9B9B), Color(hex: 0x3A1717))
        case .reply: (Color(hex: 0xF5C26B), Color(hex: 0x2A2110))
        case .decide: (Color(hex: 0xCDB6FF), Color(hex: 0x251D38))
        case .read: (Color(hex: 0x9CC4FF), Color(hex: 0x14263F))
        case .todo: (Color(hex: 0xC9CED6), Color(hex: 0x232831))
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
    static let live = "arrow.down.to.line"
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
    /// A request passed from one staff member to the next (ADR 0051).
    static let handoff = "arrow.turn.down.right"
    static let staff = "person.2"
    /// A write outside the host that a Result proposes (ADR 0052).
    static let write = "paperplane"
    /// A Task the assistant proposes, which starts in plan mode (ADR 0060).
    static let proposedTask = "hammer"
}
