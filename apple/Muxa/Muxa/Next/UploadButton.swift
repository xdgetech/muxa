import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Sends files from this device into the Task's working place (ADR 0081); each
/// path is then typed into `terminal`. iOS also offers the photo library.
struct UploadButton: View {
    @Environment(NextModel.self) private var model
    let terminal: String
    @State private var picking = false
    @State private var photo: PhotosPickerItem?
    @State private var busy = false

    var body: some View {
        menu
            .disabled(busy)
            .fileImporter(isPresented: $picking, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
                guard case .success(let urls) = result else { return }
                Task { await send(urls) }
            }
            .onChange(of: photo) { _, item in
                guard let item else { return }
                photo = nil
                Task { await send(item) }
            }
            .help("Upload a file to the Task's folder")
            .accessibilityLabel("Upload")
            .accessibilityIdentifier("focus.upload")
    }

    @ViewBuilder
    private var menu: some View {
        #if os(macOS)
        Button("Upload", systemImage: busy ? "hourglass" : "paperclip") { picking = true }
        #else
        Menu {
            PhotosPicker(selection: $photo, matching: .images) { Label("Photo", systemImage: "photo") }
            Button("File…", systemImage: "doc") { picking = true }
        } label: {
            Image(systemName: busy ? "hourglass" : "paperclip")
        }
        #endif
    }

    private func send(_ urls: [URL]) async {
        busy = true
        defer { busy = false }
        for url in urls {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            do {
                let data = try Data(contentsOf: url)
                await model.upload(data, name: url.lastPathComponent, to: terminal)
            } catch {
                model.requestError = "\(url.lastPathComponent) could not be read: \(error.localizedDescription)"
            }
        }
    }

    private func send(_ item: PhotosPickerItem) async {
        busy = true
        defer { busy = false }
        guard let data = try? await item.loadTransferable(type: Data.self) else {
            model.requestError = "The photo could not be read."
            return
        }
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let stamp = Date().formatted(.iso8601.year().month().day().time(includingFractionalSeconds: false))
            .replacingOccurrences(of: ":", with: "-")
        await model.upload(data, name: "photo-\(stamp).\(ext)", to: terminal)
    }
}

extension View {
    /// Files dropped here go to the Task's folder, and their paths into `terminal`.
    func uploadsDropped(to terminal: String, model: NextModel) -> some View {
        dropDestination(for: URL.self) { urls, _ in
            guard !urls.isEmpty else { return false }
            Task {
                for url in urls {
                    let scoped = url.startAccessingSecurityScopedResource()
                    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                    guard let data = try? Data(contentsOf: url) else {
                        model.requestError = "\(url.lastPathComponent) could not be read."
                        continue
                    }
                    await model.upload(data, name: url.lastPathComponent, to: terminal)
                }
            }
            return true
        }
    }
}
