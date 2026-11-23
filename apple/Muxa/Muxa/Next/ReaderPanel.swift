import MuxaV2
import SwiftUI

/// A file to open in the Reader.
struct ReaderTarget: Hashable {
    var kind: ReaderKind
    var path: String
    var scope: DiffScope = .uncommitted
}

/// The Reader beside a terminal (plan F2): the documents, changed files and
/// images of a Task's or a Routine run's folder.
struct ReaderPanel: View {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    @State private var kind: ReaderKind = .docs
    @State private var list: ReaderList?
    @State private var loading = false
    /// The open files, the shown one last; a link between documents adds one.
    /// Not a NavigationStack: on a phone the panel is a sheet inside Focus's
    /// stack, and a nested stack pushed its files behind the sheet.
    @State private var path: [ReaderTarget] = []

    var body: some View {
        Group {
            if terminal.taskDir == nil {
                ContentUnavailableView(
                    "No folder", systemImage: "folder",
                    description: Text("Only a Task's agent or a Routine run has files to read."))
            } else if let target = path.last {
                VStack(spacing: 0) {
                    header(target)
                    ReaderDocument(terminal: terminal.id, target: target) { follow($0, from: target) }
                        .id(target)
                        .simultaneousGesture(target.kind == .images ? swipe : nil)
                }
            } else {
                files
            }
        }
        .task(id: "\(terminal.id)/\(kind.rawValue)") { await reload() }
    }

    private func header(_ target: ReaderTarget) -> some View {
        HStack {
            Button("Back", systemImage: "chevron.backward") { path.removeLast() }
                .accessibilityIdentifier("reader.back")
            Spacer()
            Text((target.path as NSString).lastPathComponent)
                .font(.headline)
                .lineLimit(1)
                .accessibilityIdentifier("reader.title")
            Spacer()
            if target.kind == .images, let at = imageIndex(target) { imageSteps(at) }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private var files: some View {
        List {
            Picker("Show", selection: $kind) {
                Text("Docs").tag(ReaderKind.docs)
                Text("Changes").tag(ReaderKind.changes)
                Text("Images").tag(ReaderKind.images)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .accessibilityIdentifier("reader.kind")
            .listRowBackground(Color.clear)
            if let list {
                Section(list.root ?? "Folder") {
                    if list.entries.isEmpty { Text(empty).foregroundStyle(Palette.textSecondary) }
                    rows(list.entries, scope: .uncommitted)
                }
                .listRowBackground(Palette.card)
                if let branch = list.branch {
                    Section("\(branch.commits) commits since \(branch.base)") {
                        rows(branch.entries, scope: .branch)
                    }
                    .listRowBackground(Palette.card)
                }
                if list.truncated == true { Text("More files than shown.").font(.caption).foregroundStyle(Palette.textSecondary) }
            } else if loading {
                ProgressView()
            }
        }
        .groundList()
        .refreshable { await reload() }
    }

    private var empty: String {
        switch kind {
        case .docs: "No documents."
        case .changes: "No changes."
        case .images: "No images."
        }
    }

    private func rows(_ entries: [ReaderEntry], scope: DiffScope) -> some View {
        ForEach(entries) { e in
            Button {
                path.append(ReaderTarget(kind: kind, path: e.path, scope: scope))
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        if let status = e.status { Text(status.prefix(1).uppercased()).monospaced().foregroundStyle(Palette.textSecondary) }
                        Text(e.title ?? e.path).lineLimit(1)
                        Spacer()
                        if let added = e.added, let removed = e.removed {
                            Text("+\(added) −\(removed)").font(.caption).monospacedDigit().foregroundStyle(Palette.textSecondary)
                        }
                        Image(systemName: "chevron.forward").font(.caption).foregroundStyle(Palette.textTertiary)
                    }
                    if e.title != nil { Text(e.path).font(.caption2).foregroundStyle(Palette.textSecondary).lineLimit(1) }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("reader.file.\(e.path)")
        }
    }

    private var images: [ReaderEntry] { kind == .images ? list?.entries ?? [] : [] }

    private func imageIndex(_ target: ReaderTarget) -> Int? {
        images.firstIndex { $0.path == target.path }
    }

    /// `←` and `→`, or the buttons, show the previous and next image.
    private func imageSteps(_ at: Int) -> some View {
        HStack(spacing: 4) {
            Button("Previous Image", systemImage: "chevron.left") { stepImage(-1) }
                .keyboardShortcut(.leftArrow, modifiers: [])
                .disabled(at == 0)
                .accessibilityIdentifier("reader.previous")
            Text("\(at + 1) of \(images.count)")
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(Palette.textSecondary)
                .accessibilityIdentifier("reader.position")
            Button("Next Image", systemImage: "chevron.right") { stepImage(1) }
                .keyboardShortcut(.rightArrow, modifiers: [])
                .disabled(at + 1 >= images.count)
                .accessibilityIdentifier("reader.next")
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
    }

    /// A horizontal swipe moves through the images, as in Photos.
    private var swipe: some Gesture {
        DragGesture(minimumDistance: 40).onEnded { drag in
            let (dx, dy) = (drag.translation.width, drag.translation.height)
            guard abs(dx) > abs(dy) * 2 else { return }
            stepImage(dx < 0 ? 1 : -1)
        }
    }

    /// Replaces the shown image, so Back still leads to the list.
    private func stepImage(_ by: Int) {
        guard let target = path.last, target.kind == .images, let at = imageIndex(target),
              images.indices.contains(at + by) else { return }
        path[path.count - 1] = ReaderTarget(kind: .images, path: images[at + by].path)
    }

    /// A Markdown document's relative link to another listed document opens it here.
    private func follow(_ href: String, from: ReaderTarget) {
        guard from.kind == .docs, kind == .docs, let entries = list?.entries,
              let linked = ReaderLink.document(href, from: from.path, in: entries) else { return }
        path.append(ReaderTarget(kind: .docs, path: linked))
    }

    private func reload() async {
        guard terminal.taskDir != nil else { return }
        loading = true
        list = await model.readerList(terminal.id, kind: kind)
        loading = false
    }
}

/// One open file, kept current while it is shown.
struct ReaderDocument: View {
    @Environment(NextModel.self) private var model
    @Environment(\.openURL) private var openURL
    let terminal: String
    let target: ReaderTarget
    /// A link that is not a web link, by its `href`.
    var follow: (String) -> Void = { _ in }
    @State private var reader: UInt32?

    var body: some View {
        Group {
            if let reader, let reason = model.readerEnded[reader] {
                ContentUnavailableView {
                    Label("The file ended", systemImage: "doc.questionmark")
                } description: {
                    Text(reason)
                } actions: {
                    Button("Open Again") { Task { await open() } }
                }
            } else if let content = reader.flatMap({ model.readerContents[$0] }) {
                shown(content)
            } else {
                ProgressView()
            }
        }
        .task { await open() }
        .onDisappear { close() }
    }

    @ViewBuilder
    private func shown(_ content: ReaderContent) -> some View {
        switch target.kind {
        case .docs where isHTML:
            HtmlWebView(text: content.text ?? "", path: content.path)
        case .docs:
            ReaderWebView(text: content.text ?? "", path: content.path, kind: .markdown) { href, _ in link(href) }
        case .changes where content.binary == true:
            ContentUnavailableView("A binary file", systemImage: "doc", description: Text("Its diff has no text."))
        case .changes:
            ReaderWebView(text: content.text ?? "", path: content.path, kind: .diff) { href, _ in link(href) }
        case .images:
            if let image = content.bytes.flatMap(Image.init(data:)) {
                ScrollView([.horizontal, .vertical]) { image.resizable().scaledToFit() }
                    .accessibilityIdentifier("reader.image")
            } else {
                ContentUnavailableView("Not an image", systemImage: "photo")
            }
        }
    }

    private var isHTML: Bool {
        let ext = (target.path as NSString).pathExtension.lowercased()
        return ext == "html" || ext == "htm"
    }

    /// Web links open in the browser; the panel decides about the others.
    private func link(_ href: String) {
        if let url = URL(string: href), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
            openURL(url)
        } else {
            follow(href)
        }
    }

    private func open() async {
        close()
        reader = await model.openReader(terminal, kind: target.kind, path: target.path, scope: target.scope)
    }

    private func close() {
        guard let n = reader else { return }
        reader = nil
        Task { await model.closeReader(n) }
    }
}
