import ImageIO
import MuxaV2
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Sends files from this device into the Task's working place (ADR 0081); each
/// path is then typed into `terminal`. iOS also offers the photo library.
/// A failed upload shows as an alert: the Task screens have no error line.
struct UploadButton: View {
    @Environment(NextModel.self) private var model
    let terminal: String
    @State private var pickingFile = false
    @State private var pickingPhoto = false
    @State private var photo: PhotosPickerItem?
    @State private var state = UploadState.ready
    @State private var failure: String?

    var body: some View {
        menu
            .disabled(state == .busy)
            .fileImporter(isPresented: $pickingFile, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
                switch result {
                case .success(let urls): Task { await send(urls) }
                case .failure(let error): failure = error.localizedDescription
                }
            }
            // The picker as a modifier: a PhotosPicker inside a Menu closes with the menu.
            .photosPicker(isPresented: $pickingPhoto, selection: $photo, matching: .images)
            .onChange(of: photo) { _, item in
                guard let item else { return }
                photo = nil
                Task { await send(item) }
            }
            .alert("Upload failed", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
            .help("Upload a file to the Task's folder")
            .accessibilityLabel("Upload")
            .accessibilityValue(state == .sent ? "uploaded" : "")
            .accessibilityIdentifier("focus.upload")
    }

    @ViewBuilder
    private var menu: some View {
        #if os(macOS)
        Button("Upload", systemImage: state.symbol) { pickingFile = true }
        #else
        Menu {
            Button("Photo", systemImage: "photo") { pickingPhoto = true }
                .accessibilityIdentifier("upload.photo")
            Button("File…", systemImage: "doc") { pickingFile = true }
                .accessibilityIdentifier("upload.file")
        } label: {
            Image(systemName: state.symbol)
        }
        #endif
    }

    private func send(_ urls: [URL]) async {
        await sending {
            for url in urls {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                let data: Data
                do {
                    data = try Data(contentsOf: url)
                } catch {
                    return "\(url.lastPathComponent) could not be read: \(error.localizedDescription)"
                }
                if let error = await upload(data, name: url.lastPathComponent) { return error }
            }
            return nil
        }
    }

    private func send(_ item: PhotosPickerItem) async {
        await sending {
            let loaded: Data?
            do {
                loaded = try await item.loadTransferable(type: Data.self)
            } catch {
                return "The photo could not be read: \(error.localizedDescription)"
            }
            guard let loaded, let photo = UploadPhoto(loaded) else { return "The photo could not be read." }
            let stamp = Date().formatted(.iso8601.year().month().day().time(includingFractionalSeconds: false))
                .replacingOccurrences(of: ":", with: "-")
            return await upload(photo.data, name: "photo-\(stamp).\(photo.ext)")
        }
    }

    /// The error the upload left, or nil when it went through.
    private func upload(_ data: Data, name: String) async -> String? {
        model.requestError = nil
        await model.upload(data, name: name, to: terminal)
        return model.requestError
    }

    /// Runs one pick's uploads; shows a check for a moment, or the error.
    private func sending(_ work: () async -> String?) async {
        state = .busy
        if let error = await work() {
            state = .ready
            failure = error
            return
        }
        state = .sent
        try? await Task.sleep(for: .seconds(2))
        if state == .sent { state = .ready }
    }
}

private enum UploadState {
    case ready, busy, sent

    var symbol: String {
        switch self {
        case .ready: "paperclip"
        case .busy: "hourglass"
        case .sent: "checkmark"
        }
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

/// A photo as a file an agent reads. Claude Code reads PNG, JPEG, GIF and
/// WebP, not the HEIC an iPhone takes: those, and photos over the upload
/// limit, become a JPEG at most `maxSide` pixels long.
struct UploadPhoto {
    let data: Data
    let ext: String

    static let maxSide = 2048
    private static let readable: [UTType] = [.png, .jpeg, .gif, .webP]

    init?(_ data: Data) {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let type = CGImageSourceGetType(source).flatMap({ UTType($0 as String) }) else { return nil }
        if Self.readable.contains(type), data.count <= V2Client.maxUpload {
            self.init(data: data, ext: type.preferredFilenameExtension ?? "jpg")
            return
        }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Self.maxSide,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }
        self.init(data: out as Data, ext: "jpg")
    }

    private init(data: Data, ext: String) {
        self.data = data
        self.ext = ext
    }
}
