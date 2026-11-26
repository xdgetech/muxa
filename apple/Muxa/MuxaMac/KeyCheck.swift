#if DEBUG
import AppKit
import SwiftTerm

/// Debug builds only: with `-demo` and `MUXA_KEYCHECK=<log>`, the app presses
/// the Pane menu's keys as synthetic key events in each state the user
/// reported, writes one line per press to the log, and quits. It runs where
/// XCUITest cannot (another run holds UI automation, or no screen session).
@MainActor
enum KeyCheck {
    static func runIfAsked(model: NextModel, nav: NextNavigation, chrome: MacChrome) async {
        guard let path = ProcessInfo.processInfo.environment["MUXA_KEYCHECK"] else { return }
        var log: [String] = []
        func note(_ s: String) { log.append(s); NSLog("muxa keycheck: %@", s) }
        NSApp.activate(ignoringOtherApps: true)
        for _ in 0..<50 where model.terminals.isEmpty { try? await Task.sleep(for: .milliseconds(100)) }
        let kitty = ProcessInfo.processInfo.environment["MUXA_KITTY"] == "1"

        func press(_ key: String, _ flags: NSEvent.ModifierFlags = .command, code: UInt16) async {
            guard let window = chrome.mainWindow else { return note("no main window") }
            for type in [NSEvent.EventType.keyDown, .keyUp] {
                guard let e = NSEvent.keyEvent(with: type, location: .zero, modifierFlags: flags, timestamp: 0,
                                               windowNumber: window.windowNumber, context: nil, characters: key,
                                               charactersIgnoringModifiers: key, isARepeat: false, keyCode: code)
                else { continue }
                NSApp.sendEvent(e)
            }
            try? await Task.sleep(for: .milliseconds(800))
        }
        func state() -> String {
            let responder = chrome.mainWindow?.firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
            return "focus=\(nav.focus) terminal=\(nav.terminal ?? "-") panes=\(nav.panes.count) active=\(nav.activePane ?? "-") responder=\(responder)"
        }
        func enableKitty() {
            guard kitty, let pane = chrome.mainWindow?.contentView.flatMap(firstPane) else { return }
            // What Codex and Claude Code send: the kitty keyboard protocol's push.
            pane.feed(text: "\u{1b}[>1u")
        }
        func split(_ label: String) async {
            enableKitty()
            let before = nav.panes.count
            note("\(label) before ⌘D: \(state())")
            await press("d", code: 2)
            note("\(label) after ⌘D: \(state()) \(nav.panes.count > before ? "SPLIT" : "NO SPLIT")")
            // Close the new shell again, so each state starts with one pane.
            if nav.panes.count > before { await press("w", code: 13) }
        }

        nav.focus = true
        nav.terminal = "t1"
        try? await Task.sleep(for: .seconds(1))
        await split("focus")

        nav.focus = false
        nav.tab = .inbox
        try? await Task.sleep(for: .milliseconds(500))
        await press("f", [.command, .shift], code: 3)
        await split("after Home")

        nav.showReader = true
        try? await Task.sleep(for: .seconds(1))
        await split("Reader open")
        await press("\u{1b}", [], code: 53)
        try? await Task.sleep(for: .milliseconds(500))
        await split("after Reader")

        if let talk = model.terminals.first(where: { $0.id == "t7" }) {
            nav.open(talk)
            try? await Task.sleep(for: .seconds(1))
            note("Bot chat: \(state())")
            await press("f", [.command, .shift], code: 3)
            await split("after Bot chat")
        }

        nav.showChat = true
        try? await Task.sleep(for: .seconds(1))
        await split("Chat panel")
        nav.showChat = false

        for (key, code, label) in [("r", UInt16(15), "⇧⌘R"), ("p", 35, "⌘P")] {
            await press(key, key == "r" ? [.command, .shift] : .command, code: code)
            note("\(label): reader=\(nav.showReader) palette=\(nav.showPalette)")
            nav.showReader = false
            nav.showPalette = false
            try? await Task.sleep(for: .milliseconds(500))
        }
        await press("d", code: 2)
        await press("1", code: 18)
        note("⌘1: \(state())")
        if let png = ProcessInfo.processInfo.environment["MUXA_KEYCHECK_PNG"] { capture(to: png, window: chrome.mainWindow) }

        try? log.joined(separator: "\n").write(toFile: path, atomically: true, encoding: .utf8)
        exit(0)
    }

    private static func capture(to path: String, window: NSWindow?) {
        guard let view = window?.contentView?.superview ?? window?.contentView,
              let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
        view.cacheDisplay(in: view.bounds, to: rep)
        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
    }

    private static func firstPane(in view: NSView) -> MacPaneView? {
        if let pane = view as? MacPaneView { return pane }
        return view.subviews.lazy.compactMap(firstPane).first
    }
}
#endif
