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

    var body: some View {
        NavigationStack {
            Group {
                if terminal.taskDir == nil {
                    ContentUnavailableView(
                        "No folder", systemImage: "folder",
                        description: Text("Only a Task's agent or a Routine run has files to read."))
                } else {
                    files
                }
            }
            .navigationTitle("Reader")
            .navigationDestination(for: ReaderTarget.self) { target in
                ReaderDocument(terminal: terminal.id, target: target)
            }
        }
        .task(id: "\(terminal.id)/\(kind.rawValue)") { await reload() }
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
            if let list {
                Section(list.root ?? "Folder") {
                    if list.entries.isEmpty { Text(empty).foregroundStyle(.secondary) }
                    rows(list.entries, scope: .uncommitted)
                }
                if let branch = list.branch {
                    Section("\(branch.commits) commits since \(branch.base)") {
                        rows(branch.entries, scope: .branch)
                    }
                }
                if list.truncated == true { Text("More files than shown.").font(.caption).foregroundStyle(.secondary) }
            } else if loading {
                ProgressView()
            }
        }
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
            NavigationLink(value: ReaderTarget(kind: kind, path: e.path, scope: scope)) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        if let status = e.status { Text(status.prefix(1).uppercased()).monospaced().foregroundStyle(.secondary) }
                        Text(e.title ?? e.path).lineLimit(1)
                        Spacer()
                        if let added = e.added, let removed = e.removed {
                            Text("+\(added) −\(removed)").font(.caption).monospacedDigit().foregroundStyle(.secondary)
                        }
                    }
                    if e.title != nil { Text(e.path).font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
                }
            }
            .accessibilityIdentifier("reader.file.\(e.path)")
        }
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
        .navigationTitle((target.path as NSString).lastPathComponent)
        .inlineTitle()
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

    /// Web links open in the browser; other links stay in the document.
    private func link(_ href: String) {
        guard let url = URL(string: href), ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { return }
        openURL(url)
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
