import MuxaProtocol
import MuxaSession
import SwiftUI

/// The documents of the shown pane: its agent's plan first, then its repo's
/// Markdown by directory. The open one updates when its file changes.
struct ReaderScreen: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var selection: String?
    @State private var external: URL?

    var body: some View {
        NavigationSplitView {
            list
        } detail: {
            NavigationStack { detail }
        }
        .task {
            await model.loadDocs()
            selection = model.preferredDoc
        }
        .onChange(of: selection) { _, path in
            guard let path else { return }
            Task { await model.openDoc(path) }
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
        List(selection: $selection) {
            if let error = model.docError {
                Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
            }
            let plans = matches.filter { $0.kind == .plan }
            if !plans.isEmpty {
                Section("Plan") {
                    ForEach(plans) { doc in
                        DocRow(title: doc.title ?? Self.name(doc.path), detail: Self.name(doc.path), icon: "list.bullet.clipboard")
                            .tag(doc.path)
                    }
                }
            }
            ForEach(folders, id: \.folder) { group in
                Section(group.folder.isEmpty ? (model.docListing?.root ?? "Repository") : group.folder) {
                    ForEach(group.docs) { doc in
                        DocRow(title: Self.name(doc.path), detail: nil, icon: "doc.text").tag(doc.path)
                    }
                }
            }
            if model.docListing?.truncated == true {
                Text("Only the first 2000 documents are listed.").font(.caption).foregroundStyle(.secondary)
            }
        }
        .overlay {
            if model.docListing?.entries.isEmpty == true {
                ContentUnavailableView("No Documents", systemImage: "doc.text",
                                       description: Text("This pane is not in a git repository with Markdown files."))
            }
        }
        .searchable(text: $query, placement: .sidebar, prompt: "Find a document")
        .refreshable { await model.loadDocs() }
        .navigationTitle(model.docListing?.root ?? "Documents")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { model.closeReader() }
                    .keyboardShortcut(.cancelAction)
            }
        }
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
        if let doc = model.session.doc, doc.path == selection {
            if let text = doc.text {
                ReaderWebView(text: text, path: doc.path) { href, from in follow(href, from: from) }
                    .ignoresSafeArea(edges: .bottom)
                    .navigationTitle(Self.name(doc.path))
                    .navigationBarTitleDisplayMode(.inline)
                    .overlay(alignment: .bottom) {
                        if let ended = doc.ended {
                            Label("No longer updating: \(ended)", systemImage: "exclamationmark.circle")
                                .font(.callout)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                                .background(.thinMaterial, in: Capsule())
                                .padding(.bottom, 16)
                        }
                    }
            } else if let ended = doc.ended {
                ContentUnavailableView("Can't Open \(Self.name(doc.path))", systemImage: "exclamationmark.triangle",
                                       description: Text(ended))
            } else {
                ProgressView()
            }
        } else {
            ContentUnavailableView("Pick a Document", systemImage: "doc.text")
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
        selection = target
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
