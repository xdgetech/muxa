import MuxaProtocol

/// Images by folder, for the Reader's sidebar. QA runs keep their screenshots in
/// nested folders, so a tree stays short where a flat list would not.
public struct ImageFolder: Equatable, Identifiable, Sendable {
    /// The folder's path in the repo; "" for the top.
    public let path: String
    public let name: String
    /// Subfolders, then images, each in name order.
    public internal(set) var folders: [ImageFolder] = []
    public internal(set) var images: [ImageInfo] = []

    public var id: String { path }

    /// Images in this folder and all below it.
    public var count: Int { images.count + folders.reduce(0) { $0 + $1.count } }

    /// The tree of `images`. A chain of folders with one subfolder and no images
    /// is joined into one node, e.g. `build/screenshots`.
    public static func tree(_ images: [ImageInfo]) -> ImageFolder {
        var root = ImageFolder(path: "", name: "")
        for image in images {
            root.insert(image, parts: ArraySlice(image.path.split(separator: "/").map(String.init).dropLast()))
        }
        return root.sorted().joined()
    }

    /// The images next to `path`, in name order: what Previous and Next step through.
    public static func siblings(of path: String, in images: [ImageInfo]) -> [ImageInfo] {
        let folder = Self.folder(path)
        return images.filter { Self.folder($0.path) == folder }.sorted { $0.path < $1.path }
    }

    /// The folders from the top to `path`'s folder, e.g. to open them.
    public static func ancestors(of path: String) -> [String] {
        let parts = path.split(separator: "/").dropLast()
        return parts.indices.map { parts[...$0].joined(separator: "/") }
    }

    static func folder(_ path: String) -> String {
        path.split(separator: "/").dropLast().joined(separator: "/")
    }

    private mutating func insert(_ image: ImageInfo, parts: ArraySlice<String>) {
        guard let first = parts.first else {
            images.append(image)
            return
        }
        let child = path.isEmpty ? first : "\(path)/\(first)"
        if let i = folders.firstIndex(where: { $0.path == child }) {
            folders[i].insert(image, parts: parts.dropFirst())
        } else {
            var folder = ImageFolder(path: child, name: first)
            folder.insert(image, parts: parts.dropFirst())
            folders.append(folder)
        }
    }

    private func sorted() -> ImageFolder {
        var copy = self
        copy.folders = folders.map { $0.sorted() }.sorted { $0.name < $1.name }
        copy.images = images.sorted { $0.path < $1.path }
        return copy
    }

    private func joined() -> ImageFolder {
        var copy = self
        copy.folders = folders.map { folder in
            var folder = folder
            while folder.images.isEmpty, folder.folders.count == 1, let only = folder.folders.first {
                folder = ImageFolder(path: only.path, name: "\(folder.name)/\(only.name)",
                                     folders: only.folders, images: only.images)
            }
            return folder.joined()
        }
        return copy
    }
}
