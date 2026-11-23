import AppKit
import Carbon.HIToolbox
import MuxaV2
import SwiftUI

/// `⌃⌥Space` from any app opens a one-line Capture window (docs/next.md,
/// "Capture"). `⏎` adds the item and closes it; `⌘⏎` also opens Start Task
/// from it in the main window; `⎋` closes it.
@MainActor
final class QuickCapture {
    private let model: NextModel
    private let nav: NextNavigation
    private var panel: NSPanel?
    private var hotKey: EventHotKeyRef?
    private var handler: EventHandlerRef?
    /// Opens the main window; the window's scene sets it.
    var openMain: () -> Void = {}

    init(model: NextModel, nav: NextNavigation) {
        self.model = model
        self.nav = nav
    }

    /// Registers the global hotkey. Carbon's hotkeys need no Accessibility permission.
    func register() {
        guard hotKey == nil else { return }
        var type = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let me = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, _, data in
            guard let data else { return noErr }
            let capture = Unmanaged<QuickCapture>.fromOpaque(data).takeUnretainedValue()
            MainActor.assumeIsolated { capture.show() }
            return noErr
        }, 1, &type, me, &handler)
        let id = EventHotKeyID(signature: OSType(0x4D555841), id: 1) // "MUXA"
        RegisterEventHotKey(UInt32(kVK_Space), UInt32(controlKey | optionKey), id, GetApplicationEventTarget(), 0, &hotKey)
    }

    /// Each show draws a fresh view, so the field takes the keyboard again:
    /// a reused panel keeps the view, and its `onAppear` runs only once.
    func show() {
        let panel = self.panel ?? makePanel()
        self.panel = panel
        panel.contentView = NSHostingView(rootView: content())
        NSApp.activate()
        panel.center()
        panel.makeKeyAndOrderFront(nil)
    }

    func close() {
        panel?.orderOut(nil)
    }

    /// The Start Task sheet, prefilled from the capture, in the main window.
    /// A closed main window opens again first, so the sheet has a window to show in.
    private func startTask(from capture: Capture) {
        close()
        NSApp.activate()
        if let main = NSApp.windows.first(where: { !($0 is NSPanel) && $0.canBecomeMain && $0.isVisible }) {
            main.makeKeyAndOrderFront(nil)
        } else {
            openMain()
        }
        nav.newTask = TaskSeed(title: capture.title, workspace: capture.workspace)
    }

    private func makePanel() -> NSPanel {
        let panel = CapturePanel(
            contentRect: NSRect(x: 0, y: 0, width: 580, height: 110),
            styleMask: [.titled, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered, defer: false)
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.appearance = NSAppearance(named: .darkAqua)
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.onCancel = { [weak self] in self?.close() }
        panel.setAccessibilityIdentifier("quickcapture")
        return panel
    }

    private func content() -> some View {
        QuickCaptureView(
            close: { [weak self] in self?.close() },
            startTask: { [weak self] in self?.startTask(from: $0) })
        .environment(model)
    }
}

/// A panel that can take the keyboard and closes on `⎋`.
private final class CapturePanel: NSPanel {
    var onCancel: () -> Void = {}
    override var canBecomeKey: Bool { true }
    override func cancelOperation(_ sender: Any?) { onCancel() }
}

private struct QuickCaptureView: View {
    @Environment(NextModel.self) private var model
    var close: () -> Void
    var startTask: (Capture) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                LogoTile(size: 26)
                if model.isConnected {
                    CaptureField(onAdded: close, focusOnAppear: true, onStartTask: startTask, bare: true)
                } else {
                    Text("Not connected. Open muxa to connect to a host.")
                        .foregroundStyle(Palette.textSecondary)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 16)
            keys
        }
        .frame(width: 580, alignment: .leading)
        .background(Palette.surface)
        .preferredColorScheme(.dark)
    }

    /// The window's keys, as the QuickCapture mock lists them.
    private var keys: some View {
        HStack(spacing: 16) {
            key("⏎", "add")
            key("⌘⏎", "add and start a Task")
            key("esc", "close")
            Spacer()
            Text("\(Shortcut.captureAnywhere.display) anywhere")
        }
        .font(.system(size: 12))
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 18)
        .padding(.vertical, 10)
        .background(Palette.pane)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
    }

    private func key(_ key: String, _ what: String) -> some View {
        HStack(spacing: 4) {
            Text(key).font(.system(size: 12, design: .monospaced)).foregroundStyle(Palette.text)
            Text(what)
        }
    }
}
