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
    static let chat = "text.bubble"
    static let agent = "terminal"
    static let shell = "apple.terminal"
    static let fit = "textformat.size"
    static let more = "ellipsis.circle"
    static let disconnect = "xmark.circle"
    static let add = "plus"
    static let newTask = "plus.square.on.square"
    static let edit = "pencil"
    static let delete = "trash"
    static let warning = "exclamationmark.triangle"
    static let waiting = "hand.raised.fill"
    static let send = "arrow.up.circle.fill"
}
