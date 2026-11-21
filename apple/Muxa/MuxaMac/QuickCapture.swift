import AppKit
import Carbon.HIToolbox
import SwiftUI

/// `⌃⌥Space` from any app opens a one-line Capture window (docs/next.md,
/// "Capture"). `⏎` adds the item and closes it; `⎋` closes it.
@MainActor
final class QuickCapture {
    private let model: NextModel
    private var panel: NSPanel?
    private var hotKey: EventHotKeyRef?
    private var handler: EventHandlerRef?

    init(model: NextModel) {
        self.model = model
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

    func show() {
        let panel = self.panel ?? makePanel()
        self.panel = panel
        NSApp.activate()
        panel.center()
        panel.makeKeyAndOrderFront(nil)
    }

    func close() {
        panel?.orderOut(nil)
    }

    private func makePanel() -> NSPanel {
        let panel = CapturePanel(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 96),
            styleMask: [.titled, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered, defer: false)
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.onCancel = { [weak self] in self?.close() }
        panel.contentView = NSHostingView(rootView: QuickCaptureView(close: { [weak self] in self?.close() })
            .environment(model))
        panel.setAccessibilityIdentifier("quickcapture")
        return panel
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

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if model.isConnected {
                CaptureField(onAdded: close, focusOnAppear: true)
            } else {
                Text("Not connected. Open muxa to connect to a host.")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(width: 560, alignment: .leading)
    }
}
