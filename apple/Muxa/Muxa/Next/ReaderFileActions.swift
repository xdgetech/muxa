import SwiftUI
#if os(macOS)
import AppKit
#endif

/// Save (the Mac, `⌘S`) and Share for the shown Reader file. Share offers
/// Save to Files and, for an image, Save Image on iOS.
struct ReaderFileActions: View {
    let export: ReaderExport
    #if os(macOS)
    @State private var saveError: String?
    #endif

    var body: some View {
        HStack(spacing: 4) {
            #if os(macOS)
            Button("Save…", systemImage: "square.and.arrow.down") { save() }
                // A view's key, as Bot Settings' Save: the two never show together.
                .keyboardShortcut("s", modifiers: .command)
                .help("Save \(export.file.name) (⌘S)")
                .accessibilityIdentifier("reader.save")
                .alert("Could not save the file", isPresented: Binding(
                    get: { saveError != nil }, set: { if !$0 { saveError = nil } })) {
                    Button("OK") { saveError = nil }
                } message: {
                    Text(saveError ?? "")
                }
            #endif
            ShareLink(item: export.copy) {
                Label("Share", systemImage: "square.and.arrow.up")
            }
            .help("Share \(export.file.name)")
            .accessibilityIdentifier("reader.share")
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
    }

    #if os(macOS)
    /// The save panel, named as the file; the file's own bytes go where the user picks.
    private func save() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = export.file.name
        panel.canCreateDirectories = true
        panel.isExtensionHidden = false
        let file = export.file
        let done: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                try file.data.write(to: url, options: .atomic)
            } catch {
                saveError = "\(error.localizedDescription) Pick another folder and save again."
            }
        }
        if let window = NSApp.keyWindow {
            panel.beginSheetModal(for: window, completionHandler: done)
        } else {
            panel.begin(completionHandler: done)
        }
    }
    #endif
}

extension View {
    /// Drags the file out as a file: into Finder, Files, Mail or a browser's
    /// upload field, as a GitHub comment takes it. Nothing to drag without a copy.
    @ViewBuilder
    func readerFileDrag(_ export: ReaderExport?) -> some View {
        if let export {
            onDrag {
                let provider = NSItemProvider(contentsOf: export.copy) ?? NSItemProvider()
                provider.suggestedName = export.file.name
                return provider
            }
        } else {
            self
        }
    }
}
