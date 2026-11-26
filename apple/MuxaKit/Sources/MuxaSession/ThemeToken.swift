/// Every color the apps draw, by meaning (ADR 0091, appendix). The raw value
/// is the key in theme files; never rename one. Tokens are derived in this
/// order, and each rule reads only tokens above it.
public enum ThemeToken: String, CaseIterable, Sendable {
    // Base: from the terminal palette.
    case terminalBackground, terminalForeground, ground, text
    case surface, card, raised, border, textSecondary, textTertiary
    case accent, working, needsYou, done, idle, failure, decide
    // Component: from the base tokens.
    case pane, cardRaised, pill, selected, rowSelected, borderStrong, rowBorder, buttonBorder
    case divider, rowDivider, cardDivider, textMid, textDim
    case onAccent, onNeedsYou, bannerBack, bannerBorder, promptBack, promptBorder
    case destructiveSolid, destructiveBack, destructiveBorder, warnBack, warnBorder, warnText
    case infoBack, infoBorder, infoText, readText, readBack, writeText, writeBack
    case okText, okBack, failedText, decideBack, todoText, todoBack, userBubble
    case swipeDone, swipeLater, diffAdded, diffRemoved

    public var isBase: Bool { !Self.components.contains(self) }

    private static let components = Set(allCases.drop { $0 != .pane })
}

/// A terminal palette as the derivation reads it.
struct ThemePalette {
    var background: RGB
    var foreground: RGB
    var ansi: [RGB]?

    /// ANSI 1–5 when a palette has none: muxa Dark's status colors.
    static let fallback: [Int: RGB] = [
        1: RGB(0xFF, 0x9B, 0x9B), 2: RGB(0x3F, 0xC7, 0x9A), 3: RGB(0xF5, 0xA5, 0x24),
        4: RGB(0x6A, 0xA6, 0xFF), 5: RGB(0xCD, 0xB6, 0xFF),
    ]

    func ansi(_ index: Int) -> RGB {
        if let ansi, ansi.count == 16 { return ansi[index] }
        return Self.fallback[index] ?? foreground
    }
}

extension ThemeToken {
    /// The rule of the appendix's table. `v` reads a token computed earlier.
    func derive(_ palette: ThemePalette, _ v: (ThemeToken) -> RGB) -> RGB {
        let (ground, text) = (v(.ground), v(.text))
        func shade(_ p: Int) -> RGB { ground.mixed(toward: text, p) }
        func tint(_ color: ThemeToken, _ p: Int) -> RGB { ground.mixed(toward: v(color), p) }
        switch self {
        case .terminalBackground, .ground: return palette.background
        case .terminalForeground, .text: return palette.foreground
        case .surface: return shade(3)
        case .card: return shade(4)
        case .raised: return shade(8)
        case .border: return shade(11)
        case .textSecondary: return Self.dim(text, on: ground, 31, minContrast: 4.5)
        case .textTertiary: return Self.dim(text, on: ground, 42, minContrast: 3)
        case .accent: return Self.lift(palette.ansi(4), text, on: ground, minContrast: 3)
        case .working: return Self.lift(palette.ansi(2), text, on: ground, minContrast: 3)
        case .needsYou: return Self.lift(palette.ansi(3), text, on: ground, minContrast: 3)
        case .done: return v(.accent)
        case .idle: return v(.textTertiary)
        case .failure: return Self.lift(palette.ansi(1), text, on: ground, minContrast: 4.5)
        case .decide: return Self.lift(palette.ansi(5), text, on: ground, minContrast: 3)
        case .pane: return shade(2)
        case .cardRaised: return shade(5)
        case .pill: return shade(7)
        case .selected, .borderStrong: return shade(13)
        case .rowSelected, .cardDivider: return shade(9)
        case .rowBorder: return shade(18)
        case .buttonBorder: return shade(15)
        case .divider: return shade(8)
        case .rowDivider: return shade(6)
        case .textMid: return text.mixed(toward: ground, 12)
        case .textDim: return text.mixed(toward: ground, 60)
        case .onAccent: return ground
        case .onNeedsYou: return v(.needsYou).mixed(toward: .black, 90)
        case .bannerBack: return tint(.needsYou, 6)
        case .bannerBorder: return tint(.needsYou, 22)
        case .promptBack: return tint(.needsYou, 12)
        case .promptBorder: return tint(.needsYou, 28)
        case .destructiveSolid: return v(.failure).mixed(toward: .black, 20)
        case .destructiveBack: return tint(.failure, 12)
        case .destructiveBorder: return tint(.failure, 22)
        case .warnBack: return tint(.failure, 9)
        case .warnBorder: return tint(.failure, 18)
        case .warnText: return v(.failure).mixed(toward: text, 40)
        case .infoBack: return tint(.accent, 6)
        case .infoBorder: return tint(.accent, 14)
        case .infoText: return v(.accent).mixed(toward: text, 70)
        case .readText: return v(.accent).mixed(toward: text, 30)
        case .readBack: return tint(.accent, 12)
        case .writeText: return v(.needsYou).mixed(toward: text, 30)
        case .writeBack: return v(.promptBack)
        case .okText: return v(.working).mixed(toward: text, 35)
        case .okBack: return tint(.working, 12)
        case .failedText, .diffRemoved: return v(.failure)
        case .decideBack: return tint(.decide, 12)
        case .todoText: return v(.textMid)
        case .todoBack: return v(.rowSelected)
        case .userBubble: return v(.raised)
        case .swipeDone: return v(.working).mixed(toward: ground, 30)
        case .swipeLater: return v(.decide).mixed(toward: ground, 45)
        case .diffAdded: return v(.working)
        }
    }

    /// `text` mixed toward the ground by `percent`, backed off 1% at a time
    /// until it keeps `minContrast` on the ground.
    static func dim(_ text: RGB, on ground: RGB, _ percent: Int, minContrast: Double) -> RGB {
        for p in stride(from: percent, through: 0, by: -1) {
            let c = text.mixed(toward: ground, p)
            if c.contrast(with: ground) >= minContrast { return c }
        }
        return text
    }

    /// `color` mixed toward `text` 1% at a time until it has `minContrast` on the ground.
    static func lift(_ color: RGB, _ text: RGB, on ground: RGB, minContrast: Double) -> RGB {
        for p in 0...100 {
            let c = color.mixed(toward: text, p)
            if c.contrast(with: ground) >= minContrast { return c }
        }
        return text
    }
}
