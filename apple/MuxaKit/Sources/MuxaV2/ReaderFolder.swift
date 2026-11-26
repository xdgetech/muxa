/// The Reader's files under their parent folder (plan F2).
public struct ReaderFolder: Equatable, Sendable, Identifiable {
    /// From the listed folder, as `docs/adr`; empty for the listed folder itself.
    public var path: String
    public var entries: [ReaderEntry]

    public var id: String { path }

    /// Only a folder with more than one file collapses.
    public var collapses: Bool { !path.isEmpty && entries.count > 1 }

    public init(path: String, entries: [ReaderEntry]) {
        self.path = path
        self.entries = entries
    }

    /// Groups `entries` by parent folder. The folders and their files keep the
    /// list's order, so the daemon's sort still decides what comes first.
    public static func group(_ entries: [ReaderEntry]) -> [ReaderFolder] {
        var folders: [ReaderFolder] = []
        var at: [String: Int] = [:]
        for entry in entries {
            let parent = parent(of: entry.path)
            if let i = at[parent] {
                folders[i].entries.append(entry)
            } else {
                at[parent] = folders.count
                folders.append(ReaderFolder(path: parent, entries: [entry]))
            }
        }
        return folders
    }

    /// The file's name without its folder.
    public static func name(of path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    /// The folders above `path`, nearest first; never the listed folder itself.
    public static func ancestors(of path: String) -> [String] {
        var folders: [String] = []
        var at = parent(of: path)
        while !at.isEmpty {
            folders.append(at)
            at = parent(of: at)
        }
        return folders
    }

    /// The name a folder's zip is saved as: the folder's own name, or the
    /// list's root for the listed folder.
    public static func zipName(_ folder: String, root: String?) -> String {
        let base = folder.isEmpty ? root ?? "Folder" : name(of: folder)
        return base + ".zip"
    }

    /// The file's folder, as `ReaderFolder.path` names it.
    public static func parent(of path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[..<slash])
    }
}
