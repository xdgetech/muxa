import MuxaSession
import MuxaV2
import SwiftUI

/// The app's colors by meaning, from the device's theme (ADR 0091).
enum Theme {
    static var accent: Color { Palette.accent }
    static var chatBackground: Color { Palette.ground }
    static var userBubble: Color { ThemeStore.shared.color(.userBubble) }
    static var toolCard: Color { Palette.card }
    static var composer: Color { Palette.surface }
    /// Behind the pane: the terminal's own background.
    static var terminal: Color { Palette.terminal }
    static var terminalText: Color { Palette.terminalText }

    static var connected: Color { TaskState.working.color }
    static var attention: Color { TaskState.needsYou.color }
    static var failure: Color { Palette.late }
    /// ✓ closes an item (ADR 0040).
    static var done: Color { Palette.swipeDone }
    /// Later lowers an item's priority.
    static var later: Color { Palette.swipeLater }
    /// Lines a change adds and removes.
    static var added: Color { ThemeStore.shared.color(.diffAdded) }
    static var removed: Color { ThemeStore.shared.color(.diffRemoved) }
}

/// The device's theme by the mocks' names (ADR 0091, appendix). Each read goes
/// through `ThemeStore`, so a view redraws when the theme changes.
enum Palette {
    static var ground: Color { ThemeStore.shared.color(.ground) }
    static var surface: Color { ThemeStore.shared.color(.surface) }
    static var card: Color { ThemeStore.shared.color(.card) }
    /// Icon buttons and other raised controls.
    static var raised: Color { ThemeStore.shared.color(.raised) }
    /// The picked segment.
    static var selected: Color { ThemeStore.shared.color(.selected) }
    /// The picked row of a list.
    static var rowSelected: Color { ThemeStore.shared.color(.rowSelected) }
    static var border: Color { ThemeStore.shared.color(.border) }
    static var borderStrong: Color { ThemeStore.shared.color(.borderStrong) }
    /// A line between rows inside a card.
    static var divider: Color { ThemeStore.shared.color(.divider) }
    static var text: Color { ThemeStore.shared.color(.text) }
    static var textSecondary: Color { ThemeStore.shared.color(.textSecondary) }
    static var textTertiary: Color { ThemeStore.shared.color(.textTertiary) }
    static var accent: Color { ThemeStore.shared.color(.accent) }
    static var terminal: Color { ThemeStore.shared.color(.terminalBackground) }
    static var terminalText: Color { ThemeStore.shared.color(.terminalForeground) }
    /// A due date that has passed, and errors.
    static var late: Color { ThemeStore.shared.color(.failure) }
    /// The Needs-you banner on Home.
    static var bannerBack: Color { ThemeStore.shared.color(.bannerBack) }
    static var bannerBorder: Color { ThemeStore.shared.color(.bannerBorder) }
    /// The bar above a waiting Task.
    static var promptBack: Color { ThemeStore.shared.color(.promptBack) }
    static var promptBorder: Color { ThemeStore.shared.color(.promptBorder) }
    /// Text on an amber button.
    static var onAmber: Color { ThemeStore.shared.color(.onNeedsYou) }
    static var swipeLater: Color { ThemeStore.shared.color(.swipeLater) }
    static var swipeDone: Color { ThemeStore.shared.color(.swipeDone) }

    // The desktop mocks' extra tones (Mac only today; additive).
    /// Detail panes and legend strips beside the ground.
    static var pane: Color { ThemeStore.shared.color(.pane) }
    /// Cards, sheet rows and secondary buttons.
    static var cardRaised: Color { ThemeStore.shared.color(.cardRaised) }
    /// Due, branch and filter pills.
    static var pill: Color { ThemeStore.shared.color(.pill) }
    /// A selected row's border.
    static var rowBorder: Color { ThemeStore.shared.color(.rowBorder) }
    static var buttonBorder: Color { ThemeStore.shared.color(.buttonBorder) }
    static var rowDivider: Color { ThemeStore.shared.color(.rowDivider) }
    static var cardDivider: Color { ThemeStore.shared.color(.cardDivider) }
    static var textMid: Color { ThemeStore.shared.color(.textMid) }
    static var textDim: Color { ThemeStore.shared.color(.textDim) }
    static var destructive: Color { ThemeStore.shared.color(.destructiveSolid) }
    static var warnBack: Color { ThemeStore.shared.color(.warnBack) }
    static var warnBorder: Color { ThemeStore.shared.color(.warnBorder) }
    static var warnText: Color { ThemeStore.shared.color(.warnText) }
    static var infoBack: Color { ThemeStore.shared.color(.infoBack) }
    static var infoBorder: Color { ThemeStore.shared.color(.infoBorder) }
    static var infoText: Color { ThemeStore.shared.color(.infoText) }
    static var readText: Color { ThemeStore.shared.color(.readText) }
    static var readBack: Color { ThemeStore.shared.color(.readBack) }
    static var writeText: Color { ThemeStore.shared.color(.writeText) }
    static var writeBack: Color { ThemeStore.shared.color(.writeBack) }
    static var okBack: Color { ThemeStore.shared.color(.okBack) }
    static var okText: Color { ThemeStore.shared.color(.okText) }
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
/// No report is idle: muxa invents no state. `paused` is muxa's own: the Task
/// has no agent (ADR 0102), and no report maps to it.
enum TaskState: CaseIterable {
    case needsYou, done, working, idle, paused

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
        case .paused: "Paused"
        }
    }

    var color: Color {
        switch self {
        case .needsYou: ThemeStore.shared.color(.needsYou)
        case .done: ThemeStore.shared.color(.done)
        case .working: ThemeStore.shared.color(.working)
        case .idle: ThemeStore.shared.color(.idle)
        case .paused: Palette.textTertiary
        }
    }
}

extension ItemKind {
    /// The chip's text and background, as PhoneInbox's `K`. To do is neutral.
    var chipColors: (text: Color, back: Color) {
        let (text, back): (ThemeToken, ThemeToken) = switch self {
        case .review: (.okText, .okBack)
        case .failed: (.failedText, .destructiveBack)
        case .reply: (.writeText, .writeBack)
        case .decide: (.decide, .decideBack)
        case .read: (.readText, .readBack)
        case .todo: (.todoText, .todoBack)
        }
        return (ThemeStore.shared.color(text), ThemeStore.shared.color(back))
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
    static let pause = "pause.circle"
    static let resume = "play.circle"
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
