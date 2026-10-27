import MuxaProtocol
import MuxaSession
import SwiftUI

/// What the Reader shows: a document, the diff of a changed file, or an image.
enum ReaderItem: Hashable {
    case doc(String)
    case diff(String, DiffScope)
    case image(String)
}

/// The Reader sidebar's lists. Each is one tap away, however long the others are.
enum ReaderTab: String, CaseIterable, Identifiable {
    case docs, changes, images

    var id: Self { self }

    var title: LocalizedStringKey {
        switch self {
        case .docs: "Docs"
        case .changes: "Changes"
        case .images: "Images"
        }
    }
}

/// The documents of the shown pane, in three tabs: its agent's plan and its
/// repo's Markdown by directory; its uncommitted changes and its branch's
/// commits; its images as a folder tree. The open one updates when its file changes.
struct ReaderScreen: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var selection: ReaderItem?
    @State private var external: URL?
    /// Open folders of the Images tree, by path.
    @State private var expanded: Set<String> = []

    var body: some View {
        NavigationSplitView {
            list
        } detail: {
            NavigationStack { detail }
        }
        .task {
            await model.loadDocs()
            selection = model.preferredDoc.map(ReaderItem.doc)
            // The latest run's screenshots are one tap away.
            if let newest = model.imageListing?.entries.first {
                expanded.formUnion(ImageFolder.ancestors(of: newest.path))
            }
        }
        .onChange(of: selection) { _, item in
            switch item {
            case .doc(let path): Task { await model.openDoc(path) }
            case .diff(let path, let scope): Task { await model.openDiff(path, scope: scope) }
            case .image(let path): Task { await model.openImage(path) }
            case nil: break
            }
        }
        .confirmationDialog(
            "Open in Safari?", isPresented: Binding(get: { external != nil }, set: { if !$0 { external = nil } }),
            presenting: external
        ) { url in
            Button("Open \(url.host() ?? url.absoluteString)") { UIApplication.shared.open(url) }
        }
    }

    // MARK: List

    private var list: some View {
        @Bindable var model = model
        return List(selection: $selection) {
            if let error = model.docError {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
            }
            switch model.readerTab {
            case .docs: docRows
            case .changes: changeRows
            case .images: imageRows
            }
        }
        .overlay { emptyState }
        .safeAreaInset(edge: .top) {
            if model.readerTabs.count > 1 {
                Picker("Show", selection: $model.readerTab) {
                    ForEach(model.readerTabs) { tab in
                        Text(tab.title).tag(tab)
                    }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 16)
                .padding(.bottom, 6)
                .accessibilityIdentifier("reader-tab")
            }
        }
        .searchable(text: $query, placement: .sidebar, prompt: prompt)
        .refreshable { await model.loadDocs() }
        .navigationTitle(model.docListing?.root ?? "Documents")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { model.closeReader() }
                    .keyboardShortcut(.cancelAction)
            }
        }
    }

    @ViewBuilder
    private var docRows: some View {
        let plans = matches.filter { $0.kind == .plan }
        if !plans.isEmpty {
            Section("Plan") {
                ForEach(plans) { doc in
                    DocRow(title: doc.title ?? Self.name(doc.path), detail: Self.name(doc.path), icon: Icon.plan)
                        .tag(ReaderItem.doc(doc.path))
                }
            }
        }
        ForEach(folders, id: \.folder) { group in
            Section(group.folder.isEmpty ? (model.docListing?.root ?? "Repository") : group.folder) {
                ForEach(group.docs) { doc in
                    DocRow(title: Self.name(doc.path), detail: nil, icon: Icon.document).tag(ReaderItem.doc(doc.path))
                }
            }
        }
        if model.docListing?.truncated == true {
            Text("Only the first 2000 documents are listed.").font(.caption).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var changeRows: some View {
        if !changes.isEmpty {
            Section("Uncommitted") {
                ForEach(changes) { change in
                    ChangeRow(change: change, scope: .uncommitted).tag(ReaderItem.diff(change.path, .uncommitted))
                }
                if model.changeListing?.truncated == true {
                    Text("Only the first 2000 changed files are listed.").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        if let branch = model.changeListing?.branch, !branchChanges.isEmpty {
            Section {
                ForEach(branchChanges) { change in
                    ChangeRow(change: change, scope: .branch).tag(ReaderItem.diff(change.path, .branch))
                }
                if branch.truncated {
                    Text("Only the first 2000 changed files are listed.").font(.caption).foregroundStyle(.secondary)
                }
            } header: {
                Text(branch.commits == 1 ? "Since \(branch.base) · 1 commit" : "Since \(branch.base) · \(branch.commits) commits")
                    .accessibilityIdentifier("branch-header")
            }
        }
    }

    @ViewBuilder
    private var imageRows: some View {
        if !images.isEmpty {
            ImageFolderRows(folder: ImageFolder.tree(images), expanded: query.isEmpty ? $expanded : .constant(allFolders))
            if model.imageListing?.truncated == true {
                Text("Only the newest 2000 images are listed.").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    /// Why the open tab is empty, once its list arrived.
    @ViewBuilder
    private var emptyState: some View {
        switch model.readerTab {
        case .docs where model.docListing?.entries.isEmpty == true:
            ContentUnavailableView("No Documents", systemImage: Icon.document,
                                   description: Text("This pane is not in a git repository with Markdown files."))
        case .changes where model.changeListing != nil && changes.isEmpty && branchChanges.isEmpty:
            ContentUnavailableView("No Changes", systemImage: Icon.document,
                                   description: Text("Nothing is uncommitted, and the branch has no commits of its own."))
        case .images where model.imageListing?.entries.isEmpty == true:
            ContentUnavailableView("No Images", systemImage: Icon.image,
                                   description: Text("This pane's git repository has no image files."))
        default:
            EmptyView()
        }
    }

    private var prompt: LocalizedStringKey {
        switch model.readerTab {
        case .docs: "Find a document"
        case .changes: "Find a changed file"
        case .images: "Find an image"
        }
    }

    private var branchChanges: [ChangeInfo] {
        let all = model.changeListing?.branch?.entries ?? []
        guard !query.isEmpty else { return all }
        return all.filter { $0.path.localizedCaseInsensitiveContains(query) }
    }

    private var changes: [ChangeInfo] {
        let all = model.changeListing?.entries ?? []
        guard !query.isEmpty else { return all }
        return all.filter { $0.path.localizedCaseInsensitiveContains(query) }
    }

    private var images: [ImageInfo] {
        let all = model.imageListing?.entries ?? []
        guard !query.isEmpty else { return all }
        return all.filter { $0.path.localizedCaseInsensitiveContains(query) }
    }

    /// While searching, every folder with a match is open.
    private var allFolders: Set<String> {
        Set(images.flatMap { ImageFolder.ancestors(of: $0.path) })
    }

    private var matches: [DocInfo] {
        let all = model.docListing?.entries ?? []
        guard !query.isEmpty else { return all }
        return all.filter { $0.path.localizedCaseInsensitiveContains(query) || ($0.title ?? "").localizedCaseInsensitiveContains(query) }
    }

    /// Repo documents by folder, the top folder first, in path order.
    private var folders: [(folder: String, docs: [DocInfo])] {
        let repo = matches.filter { $0.kind == .repo }
        let grouped = Dictionary(grouping: repo) { Self.folder($0.path) }
        return grouped.keys.sorted().map { ($0, grouped[$0] ?? []) }
    }

    static func name(_ path: String) -> String { path.split(separator: "/").last.map(String.init) ?? path }

    static func folder(_ path: String) -> String {
        let parts = path.split(separator: "/")
        return parts.dropLast().joined(separator: "/")
    }

    // MARK: Detail

    @ViewBuilder
    private var detail: some View {
        switch selection {
        case .doc(let path): docDetail(path)
        case .diff(let path, let scope): diffDetail(path, scope: scope)
        case .image(let path): ImageDetail(path: path, selection: $selection)
        case nil: ContentUnavailableView("Pick a Document", systemImage: Icon.document)
        }
    }

    @ViewBuilder
    private func diffDetail(_ path: String, scope: DiffScope) -> some View {
        if let diff = model.session.diff, diff.path == path, diff.scope == scope {
            Group {
                if let text = diff.text {
                    ReaderWebView(text: text, path: path, kind: .diff) { _, _ in }
                        .ignoresSafeArea(edges: .bottom)
                } else if diff.binary {
                    ContentUnavailableView("Binary File", systemImage: Icon.document,
                                           description: Text("Changes to this file cannot be shown as text."))
                } else if let ended = diff.ended {
                    ContentUnavailableView("No Diff for \(Self.name(path))", systemImage: Icon.warning,
                                           description: Text(ended))
                } else {
                    ProgressView()
                }
            }
            .navigationTitle(Self.name(path))
            .navigationBarTitleDisplayMode(.inline)
            .overlay(alignment: .bottom) {
                if diff.loaded, let ended = diff.ended {
                    EndedNotice(text: ended)
                }
            }
        } else {
            ProgressView()
        }
    }

    @ViewBuilder
    private func docDetail(_ selected: String) -> some View {
        if let doc = model.session.doc, doc.path == selected {
            if let text = doc.text {
                ReaderWebView(text: text, path: doc.path) { href, from in follow(href, from: from) }
                    .ignoresSafeArea(edges: .bottom)
                    .navigationTitle(Self.name(doc.path))
                    .navigationBarTitleDisplayMode(.inline)
                    .overlay(alignment: .bottom) {
                        if let ended = doc.ended { EndedNotice(text: ended) }
                    }
            } else if let ended = doc.ended {
                ContentUnavailableView("Can't Open \(Self.name(doc.path))", systemImage: Icon.warning,
                                       description: Text(ended))
            } else {
                ProgressView()
            }
        } else {
            ProgressView()
        }
    }

    /// A relative link to a listed document opens here; a web link asks first;
    /// anything else is not followed.
    private func follow(_ href: String, from: String) {
        if let url = URL(string: href), let scheme = url.scheme, ["http", "https"].contains(scheme) {
            external = url
            return
        }
        guard let target = Self.resolve(href, from: from),
            model.docListing?.entries.contains(where: { $0.path == target }) == true
        else { return }
        selection = .doc(target)
    }

    /// `href` relative to the folder of `from`, without its fragment or query.
    static func resolve(_ href: String, from: String) -> String? {
        let bare = String(href.split(separator: "#", maxSplits: 1).first ?? "").split(separator: "?").first.map(String.init) ?? ""
        guard !bare.isEmpty, !bare.contains(":") else { return nil }
        var parts = bare.hasPrefix("/") ? [] : folder(from).split(separator: "/").map(String.init)
        for part in bare.split(separator: "/").map(String.init) {
            switch part {
            case ".", "": continue
            case "..":
                guard !parts.isEmpty else { return nil }
                parts.removeLast()
            default: parts.append(part.removingPercentEncoding ?? part)
            }
        }
        return parts.joined(separator: "/")
    }
}

/// Shown over a document, diff or image that stopped updating.
struct EndedNotice: View {
    let text: String

    var body: some View {
        Label("No longer updating: \(text)", systemImage: "exclamationmark.circle")
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(.thinMaterial, in: Capsule())
            .padding(.bottom, 16)
    }
}

/// A changed file: its status letter, name and folder, and the lines it adds and removes.
private struct ChangeRow: View {
    let change: ChangeInfo
    let scope: DiffScope

    var body: some View {
        HStack(spacing: 10) {
            Text(letter)
                .font(.caption.monospaced().weight(.bold))
                .foregroundStyle(color)
                .frame(width: 18, height: 18)
                .background(color.opacity(0.15), in: RoundedRectangle(cornerRadius: 4))
                .accessibilityLabel(change.status.rawValue)
            VStack(alignment: .leading, spacing: 1) {
                Text(ReaderScreen.name(change.path)).lineLimit(1)
                let folder = ReaderScreen.folder(change.path)
                if let old = change.oldPath {
                    Text("from \(old)").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                } else if !folder.isEmpty {
                    Text(folder).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer(minLength: 4)
            if let added = change.added, let removed = change.removed {
                HStack(spacing: 4) {
                    Text("+\(added)").foregroundStyle(Theme.added)
                    Text("−\(removed)").foregroundStyle(Theme.removed)
                }
                .font(.caption.monospacedDigit())
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(scope == .branch ? "branch-change-\(change.path)" : "change-\(change.path)")
    }

    private var letter: String {
        switch change.status {
        case .modified: "M"
        case .added: "A"
        case .deleted: "D"
        case .renamed: "R"
        case .untracked: "U"
        case .conflicted: "!"
        case .other: "?"
        }
    }

    private var color: Color {
        switch change.status {
        case .added, .untracked: Theme.added
        case .deleted, .conflicted: Theme.removed
        case .modified, .renamed, .other: Theme.attention
        }
    }
}

private struct DocRow: View {
    let title: String
    let detail: String?
    let icon: String

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 1) {
                Text(title).lineLimit(2)
                if let detail, detail != title {
                    Text(detail).font(.caption).foregroundStyle(.secondary)
                }
            }
        } icon: {
            Image(systemName: icon)
        }
    }
}
