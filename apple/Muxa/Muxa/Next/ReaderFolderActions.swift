import MuxaV2
import SwiftUI
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#endif

/// The host refused or could not send a folder; `message` is for the user.
struct ReaderFolderError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// A Reader folder as the zip the host makes of its listed files (ADR 0101).
/// It is fetched only when the user shares, drags or saves it.
struct ReaderFolderZip: Transferable {
    let name: String
    let fetch: @Sendable () async throws -> Data

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .zip) { zip in
            SentTransferredFile(try ReaderFile(name: zip.name, data: try await zip.fetch()).writeCopy())
        }
        .suggestedFileName { $0.name }
    }
}

/// Save Folder… (the Mac) and Share Folder for one folder of the Reader's list.
struct ReaderFolderActions: View {
    @Environment(NextModel.self) private var model
    let folder: String
    let zip: ReaderFolderZip

    var body: some View {
        #if os(macOS)
        Button("Save Folder…", systemImage: "square.and.arrow.down") { save() }
            .accessibilityIdentifier("reader.saveFolder.\(folder)")
        #endif
        ShareLink(item: zip, preview: SharePreview(zip.name)) {
            Label("Share Folder", systemImage: "square.and.arrow.up")
        }
        .accessibilityIdentifier("reader.shareFolder.\(folder)")
    }

    #if os(macOS)
    /// The save panel, named as the zip; the zip is fetched once the user picks a place.
    private func save() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = zip.name
        panel.allowedContentTypes = [.zip]
        panel.canCreateDirectories = true
        panel.isExtensionHidden = false
        let (fetch, model) = (zip.fetch, model)
        let done: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor in
                do {
                    try await fetch().write(to: url, options: .atomic)
                } catch is ReaderFolderError {
                    // The model has the host's words already.
                } catch {
                    model.readerFolderFailure = "\(error.localizedDescription) Pick another folder and save again."
                }
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
    /// A folder row's Save Folder… and Share Folder, and its drag as the zip.
    @ViewBuilder
    func readerFolderActions(_ folder: String, zip: ReaderFolderZip, when shown: Bool) -> some View {
        if shown {
            contextMenu { ReaderFolderActions(folder: folder, zip: zip) }
                .draggable(zip)
        } else {
            self
        }
    }
}
