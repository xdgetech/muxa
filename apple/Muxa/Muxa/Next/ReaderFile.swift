import Foundation
import MuxaV2
import UniformTypeIdentifiers

/// A Reader file as the user saves, shares or drags it: the file's own bytes,
/// not the rendered page (ADR 0100). A diff is not a file in the folder; it
/// goes out as `<name>.diff`. A binary file's diff has no text and no file.
struct ReaderFile: Equatable, Sendable {
    let name: String
    let data: Data

    init?(kind: ReaderKind, content: ReaderContent) {
        let name = Self.fileName(content.path)
        switch kind {
        case .docs:
            guard let text = content.text else { return nil }
            self.init(name: name, data: Data(text.utf8))
        case .changes:
            guard content.binary != true, let text = content.text else { return nil }
            self.init(name: name + ".diff", data: Data(text.utf8))
        case .images:
            guard let bytes = content.bytes else { return nil }
            self.init(name: name, data: bytes)
        }
    }

    init(name: String, data: Data) {
        self.name = name
        self.data = data
    }

    var type: UTType { UTType(filenameExtension: (name as NSString).pathExtension) ?? .data }

    /// The last part of a listed path; never empty, `.` or `..`.
    static func fileName(_ path: String) -> String {
        let last = (path as NSString).lastPathComponent
        return ["", ".", "..", "/"].contains(last) ? "file" : last
    }

    /// Writes a copy in a new folder of its own under the temporary folder.
    /// Share and drag hand out its URL, so the receiver sees the file's name.
    func writeCopy() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("Reader", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent(name)
        try data.write(to: url, options: .atomic)
        return url
    }

    /// Removes a copy `writeCopy` made, with its folder. A copy left behind
    /// costs nothing: the system clears the temporary folder.
    static func removeCopy(_ url: URL) {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }
}

/// The shown file and its copy on disk.
struct ReaderExport: Equatable {
    let file: ReaderFile
    let copy: URL
}
