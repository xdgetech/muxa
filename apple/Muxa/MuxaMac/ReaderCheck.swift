#if DEBUG
import AppKit
import MuxaSession
import SwiftUI

/// Debug builds only: with `-demo` and `MUXA_READERCHECK=<log>`, the app shows
/// the Reader in a plain AppKit window, right-clicks and drags each row of its
/// Docs and Images lists with synthetic mouse events, logs the menus and the
/// drag pasteboard's types, and quits. `MUXA_READER_PLACE=bots` checks a Bot's
/// Reader instead of Focus's. It runs without a screen session, where the app's
/// own window and XCUITest do not open.
@MainActor
enum ReaderCheck {
    private static var keys: WindowKeyMonitor?

    static func runIfAsked() {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["MUXA_READERCHECK"] else { return }
        let model = NextModel(hosts: HostStore(file: FileManager.default.temporaryDirectory.appendingPathComponent("readercheck-hosts.json")))
        let nav = NextNavigation()
        let chrome = MacChrome()
        let window = mainWindow(model: model, nav: nav, chrome: chrome)
        keys = WindowKeyMonitor(model: model, nav: nav, chrome: chrome)
        keys?.install()
        Task {
            await model.startDemo()
            for _ in 0..<50 where model.terminals.isEmpty { try? await Task.sleep(for: .milliseconds(100)) }
            if env["MUXA_READER_PLACE"] == "bots" {
                nav.tab = .bots
                let bot = BotPick.bot(workspace: "app", name: "pr-watch")
                chrome.botPick = bot
                chrome.botTab = .chats
                chrome.botTabs[bot] = .chats
                chrome.botRoutedTo = bot
                chrome.botTerminal = "t7"
                try? await Task.sleep(for: .seconds(1))
                chrome.botView = .reader
            } else {
                nav.focus = true
                nav.terminal = "t2"
                try? await Task.sleep(for: .seconds(1))
                nav.showReader = true
            }
            try? await Task.sleep(for: .seconds(2))
            let log = await check(window)
            do {
                try log.joined(separator: "\n").appending("\n").write(toFile: path, atomically: true, encoding: .utf8)
                exit(0)
            } catch {
                NSLog("muxa readercheck: could not write %@: %@", path, error.localizedDescription)
                exit(1)
            }
        }
    }

    /// The main window's root, as `MuxaMacApp` builds it, in an AppKit window.
    private static func mainWindow(model: NextModel, nav: NextNavigation, chrome: MacChrome) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1440, height: 900),
                              styleMask: [.titled, .resizable, .closable, .miniaturizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        let root = NextRoot()
            .environment(model)
            .environment(nav)
            .environment(chrome)
            .modifier(ThemedWindow())
            .frame(minWidth: 720, minHeight: 480)
            .sheet(isPresented: Bindable(chrome).showShortcuts) { ShortcutsSheet() }
            .background(WindowReader { chrome.mainWindow = $0 })
            .toolbar(.hidden, for: .windowToolbar)
            .ignoresSafeArea(.container, edges: .top)
        window.contentView = NSHostingView(rootView: root)
        window.makeKeyAndOrderFront(nil)
        return window
    }

    private static func check(_ window: NSWindow) async -> [String] {
        guard list(window) != nil else { return ["no Reader list"] }
        var log = ["docs:"] + (await rows(window))
        guard let picker = list(window).flatMap({ first(NSSegmentedControl.self, in: $0) }) else { return log + ["no kind picker"] }
        picker.selectedSegment = 2
        _ = picker.sendAction(picker.action, to: picker.target)
        try? await Task.sleep(for: .seconds(2))
        log += ["images:"] + (await rows(window))
        return log
    }

    /// Right-clicks each row of the list and drags each folder row. A drag
    /// on a file row would open the file, as a click does.
    private static func rows(_ window: NSWindow) async -> [String] {
        guard let table = list(window) else { return ["no Reader list"] }
        var lines: [String] = []
        for row in 0..<table.numberOfRows {
            let at = center(of: row, in: table)
            var line = "row \(row) menu=\(await rightClick(at, in: window))"
            if table.isExpandable(table.item(atRow: row)) {
                line += " folder drag=\(await drag(at, in: window))"
            }
            lines.append(line)
        }
        return lines
    }

    /// The Reader's list: the one with the Docs, Changes and Images picker; the sidebar is a list too.
    private static func list(_ window: NSWindow) -> NSOutlineView? {
        guard let root = window.contentView else { return nil }
        return all(NSOutlineView.self, in: root).first { first(NSSegmentedControl.self, in: $0) != nil }
    }

    private static func center(of row: Int, in table: NSOutlineView) -> NSPoint {
        let rect = table.rect(ofRow: row)
        return table.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    }

    private static func event(_ type: NSEvent.EventType, _ at: NSPoint, in window: NSWindow) -> NSEvent? {
        NSEvent.mouseEvent(with: type, location: at, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                           windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)
    }

    /// The titles of the menu a right-click opens, closed at once.
    private static func rightClick(_ at: NSPoint, in window: NSWindow) async -> [String] {
        var opened: [String] = []
        let observer = NotificationCenter.default.addObserver(forName: NSMenu.didBeginTrackingNotification, object: nil, queue: nil) { note in
            guard let menu = note.object as? NSMenu else { return }
            opened = menu.items.map(\.title)
            RunLoop.main.perform(inModes: [.eventTracking, .common]) { menu.cancelTrackingWithoutAnimation() }
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        for type in [NSEvent.EventType.rightMouseDown, .rightMouseUp] {
            event(type, at, in: window).map(window.sendEvent)
        }
        try? await Task.sleep(for: .milliseconds(300))
        return opened
    }

    /// The content types a 60 pt mouse drag from `at` puts on the drag pasteboard.
    private static func drag(_ at: NSPoint, in window: NSWindow) async -> [String] {
        let board = NSPasteboard(name: .drag)
        let before = board.changeCount
        // The press runs AppKit's tracking loop, which reads the queued drag and release.
        for dx in stride(from: 10, through: 60, by: 10) {
            event(.leftMouseDragged, NSPoint(x: at.x + CGFloat(dx), y: at.y), in: window).map { NSApp.postEvent($0, atStart: false) }
        }
        event(.leftMouseUp, NSPoint(x: at.x + 60, y: at.y), in: window).map { NSApp.postEvent($0, atStart: false) }
        event(.leftMouseDown, at, in: window).map(window.sendEvent)
        try? await Task.sleep(for: .milliseconds(500))
        guard board.changeCount != before else { return [] }
        return (board.types ?? []).map(\.rawValue).filter { $0.hasPrefix("public.") }
    }

    private static func all<T: NSView>(_ type: T.Type, in view: NSView) -> [T] {
        ((view as? T).map { [$0] } ?? []) + view.subviews.flatMap { all(type, in: $0) }
    }

    private static func first<T: NSView>(_ type: T.Type, in view: NSView) -> T? {
        if let match = view as? T { return match }
        return view.subviews.lazy.compactMap { first(type, in: $0) }.first
    }
}
#endif
