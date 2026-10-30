// Puts each screenshot under a caption, at the exact size of one store slot.
// Run: swift apple/Muxa/Art/frame-screenshots.swift <ipad|iphone|play-phone|play-tablet> <screenshot dir> <output dir>
// Writes one folder per locale: <output dir>/en, <output dir>/ja.
import AppKit
import CoreGraphics

/// A store slot: the size the store accepts, and what its captions may assume.
struct Target {
    let canvas: CGSize
    /// Captions that differ from the iPad's, which names ⌘ keys and the iPad.
    let overrides: [String: [String: (String, String)]]
}

/// Phones without a keyboard: taps, not shortcuts.
let touch: [String: [String: (String, String)]] = [
    "en": [
        "3-switcher": ("Every agent, one tap away", "See who is running, waiting or done."),
        "4-plan": ("Read the plans your agents write", "Markdown and diagrams, rendered offline."),
        "6-shell": ("The shell is one tap away", "Switch between the agent and its shell."),
    ],
    "ja": [
        "1-chat": ("いつものエージェントと、手元で話す", "tmuxで動くエージェントを、ネイティブのチャットで。"),
        "3-switcher": ("どのエージェントにも、タップ1つで", "実行中・回答待ち・完了がひと目でわかる。"),
        "4-plan": ("エージェントが書いたプランを読む", "Markdownも図も、オフラインで表示。"),
        "6-shell": ("シェルもタップ1つで", "エージェントとシェルを行き来。"),
    ],
]

let targets: [String: Target] = [
    // App Store 13-inch iPad, landscape.
    "ipad": Target(canvas: CGSize(width: 2752, height: 2064), overrides: [:]),
    // App Store 6.9-inch iPhone, portrait.
    "iphone": Target(canvas: CGSize(width: 1320, height: 2868), overrides: touch),
    // Google Play phone: 9:16, at least 1080 px.
    "play-phone": Target(canvas: CGSize(width: 1080, height: 1920), overrides: touch),
    // Google Play 7- and 10-inch tablets: 16:9, with Android's Ctrl+Shift chords.
    "play-tablet": Target(canvas: CGSize(width: 2560, height: 1440), overrides: [
        "en": [
            "3-switcher": ("Every agent, one keystroke away", "Ctrl+Shift+K shows who is running, waiting or done."),
            "4-plan": ("Read the plans your agents write", "Markdown and diagrams, rendered on the tablet."),
            "6-shell": ("The shell is one key away", "Ctrl+Shift+J switches between the agent and its shell."),
        ],
        "ja": [
            "1-chat": ("いつものエージェントと、タブレットで話す", "tmuxで動くエージェントを、ネイティブのチャットで。"),
            "3-switcher": ("どのエージェントにも、キー1つで", "Ctrl+Shift+Kで、実行中・回答待ち・完了がひと目でわかる。"),
            "4-plan": ("エージェントが書いたプランを読む", "Markdownも図も、タブレットの上で表示。"),
            "6-shell": ("シェルもキー1つで", "Ctrl+Shift+Jで、エージェントとシェルを行き来。"),
        ],
    ]),
]

guard CommandLine.arguments.count == 4, let target = targets[CommandLine.arguments[1]] else {
    let names = targets.keys.sorted().joined(separator: "|")
    FileHandle.standardError.write(Data("usage: frame-screenshots.swift <\(names)> <screenshot dir> <output dir>\n".utf8))
    exit(2)
}
let canvas = target.canvas
/// Sizes below are the iPad's; smaller slots scale them by their short side.
let scale = min(canvas.width, canvas.height) / 2064
/// A tall slot has room for bigger captions than its width alone would give.
let captionScale = canvas.height > canvas.width ? canvas.width / 1500 : scale

/// File names without extension, in store order. Files not listed are skipped.
let order = ["1-chat", "2-agent", "3-switcher", "7-alert", "4-plan", "5-diff", "6-shell"]

/// Caption and subtitle per screenshot, per locale, as the iPad shows them.
let captions: [String: [String: (String, String)]] = [
    "en": [
        "1-chat": ("Talk to the agents you already run", "The agents in your tmux, as a native chat."),
        "2-agent": ("The real pane, at its real size", "Your desktop layout never changes."),
        "3-switcher": ("Every agent, one keystroke away", "⌘K shows who is running, waiting or done."),
        "7-alert": ("Know when an agent needs you", "A banner when another session waits for your answer."),
        "4-plan": ("Read the plans your agents write", "Markdown and diagrams, rendered on the iPad."),
        "5-diff": ("Review changes as they happen", "Uncommitted diffs, updated while the agent edits."),
        "6-shell": ("The shell is one key away", "⌘J switches between the agent and its shell."),
    ],
    "ja": [
        "1-chat": ("いつものエージェントと、iPadで話す", "tmuxで動くエージェントを、ネイティブのチャットで。"),
        "2-agent": ("本物のペインを、本来のサイズで", "デスクトップのレイアウトは一切変わりません。"),
        "3-switcher": ("どのエージェントにも、キー1つで", "⌘Kで、実行中・回答待ち・完了がひと目でわかる。"),
        "7-alert": ("エージェントに呼ばれたら、すぐわかる", "別のセッションが回答を待つと、バナーで知らせます。"),
        "4-plan": ("エージェントが書いたプランを読む", "Markdownも図も、iPadの上で表示。"),
        "5-diff": ("変更を、その場でレビュー", "エージェントの編集に合わせて、差分も更新。"),
        "6-shell": ("シェルもキー1つで", "⌘Jで、エージェントとシェルを行き来。"),
    ],
]

func rgb(_ r: Int, _ g: Int, _ b: Int, _ a: Double = 1) -> CGColor {
    CGColor(srgbRed: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255, alpha: a)
}

enum FrameError: Error {
    case unreadable(URL)
    case unwritable(URL)
}

func frame(_ shot: CGImage, title: String, subtitle: String) -> CGImage {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: Int(canvas.width), height: Int(canvas.height), bitsPerComponent: 8,
                        bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    // The icon's indigo, so the store page and the icon read as one.
    let bg = CGGradient(colorsSpace: space, colors: [rgb(0x86, 0x83, 0xFF), rgb(0x5E, 0x5C, 0xE6), rgb(0x3F, 0x37, 0xC2)] as CFArray,
                        locations: [0, 0.45, 1])!
    ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: canvas.height), end: CGPoint(x: canvas.width, y: 0), options: [])

    // The caption flows down from the top: a narrow slot wraps it onto more lines.
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
    let titleBottom = draw(title, size: 112 * captionScale, weight: .bold, alpha: 1, top: 150 * scale)
    let subtitleBottom = draw(subtitle, size: 60 * captionScale, weight: .medium, alpha: 0.82,
                              top: max(300 * scale, titleBottom + 16 * scale))
    NSGraphicsContext.current = nil

    // The screenshot fills the lower part and runs off the bottom edge.
    let top = max(470 * scale, subtitleBottom + 60 * scale)
    let width = canvas.width * 0.84
    let height = width * Double(shot.height) / Double(shot.width)
    let rect = CGRect(x: (canvas.width - width) / 2, y: canvas.height - top - height, width: width, height: height)
    let path = CGPath(roundedRect: rect, cornerWidth: 44 * scale, cornerHeight: 44 * scale, transform: nil)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -24 * scale), blur: 80 * scale, color: rgb(0x14, 0x10, 0x5C, 0.5))
    ctx.addPath(path)
    ctx.setFillColor(rgb(0, 0, 0))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    ctx.draw(shot, in: rect)
    ctx.restoreGState()
    return ctx.makeImage()!
}

/// Centred text whose top edge is `top` points below the canvas's top. Returns its bottom edge.
/// It shrinks to stay on one line, to 70% of `size`, before it wraps: Japanese wraps mid-word.
@discardableResult
func draw(_ text: String, size: Double, weight: NSFont.Weight, alpha: Double, top: Double) -> Double {
    let width = canvas.width * 0.9
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    func styled(_ points: Double) -> NSAttributedString {
        NSAttributedString(string: text, attributes: [
            .font: NSFont.systemFont(ofSize: points, weight: weight),
            .foregroundColor: NSColor(white: 1, alpha: alpha),
            .paragraphStyle: style,
        ])
    }
    var points = size
    while points > size * 0.7, styled(points).size().width > width {
        points -= size * 0.05
    }
    let string = styled(points)
    let height = ceil(string.boundingRect(with: CGSize(width: width, height: .greatestFiniteMagnitude),
                                          options: .usesLineFragmentOrigin).height)
    string.draw(with: CGRect(x: (canvas.width - width) / 2, y: canvas.height - top - height, width: width, height: height),
                options: .usesLineFragmentOrigin)
    return top + height
}

func load(_ url: URL) throws -> CGImage {
    guard let image = NSImage(contentsOf: url)?.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        throw FrameError.unreadable(url)
    }
    return image
}

func save(_ image: CGImage, to url: URL) throws {
    guard let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
        throw FrameError.unwritable(url)
    }
    try png.write(to: url)
}

let input = URL(fileURLWithPath: CommandLine.arguments[2])
let output = URL(fileURLWithPath: CommandLine.arguments[3])
do {
    for (locale, texts) in captions {
        let dir = output.appendingPathComponent(locale)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        for (index, name) in order.enumerated() {
            guard let (title, subtitle) = target.overrides[locale]?[name] ?? texts[name] else {
                FileHandle.standardError.write(Data("frame-screenshots: no \(locale) caption for \(name)\n".utf8))
                exit(1)
            }
            let shot = try load(input.appendingPathComponent("\(name).png"))
            // Store order, not capture order.
            try save(frame(shot, title: title, subtitle: subtitle),
                     to: dir.appendingPathComponent("\(index + 1)-\(name.dropFirst(2)).png"))
        }
    }
} catch {
    FileHandle.standardError.write(Data("frame-screenshots: \(error)\n".utf8))
    exit(1)
}
