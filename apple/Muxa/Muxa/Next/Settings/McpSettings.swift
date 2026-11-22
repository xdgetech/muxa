import MuxaV2
import SwiftUI

/// The Workspace's MCP servers, and who used each when.
struct WorkspaceMcpForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: McpDraft?

    var body: some View {
        SettingsPage {
            Section { EveryAgentNote() }
            list
        }
        .sheet(item: $editing) { McpEditor(workspace: workspace, draft: $0) }
    }

    private var list: some View {
        Section("MCP servers") {
            ForEach(workspace.mcp) { m in
                let used = model.settings.usage[workspace.name]?.mcp.first { $0.name == m.name }
                Button { editing = McpDraft(m) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 8) {
                            Text(m.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                            Spacer()
                            StateTag(text: m.url == nil ? "command" : (m.transport ?? "http"))
                        }
                        Text(Self.detail(m)).monospaced().font(.caption).foregroundStyle(Palette.textSecondary)
                            .lineLimit(2)
                        UsageLine(usedBy: used?.usedBy, lastUsed: used?.lastUsed)
                    }
                    .padding(.vertical, 2)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("mcp.\(m.name)")
            }
            Button("Add MCP Server", systemImage: "plus") { editing = McpDraft() }
                .accessibilityIdentifier("mcp.add")
        }
    }

    static func detail(_ m: McpSpec) -> String {
        var out = m.url ?? ([m.command ?? ""] + (m.args ?? [])).joined(separator: " ")
        if !m.connections.isEmpty { out += " · needs " + m.connections.joined(separator: ", ") }
        return out
    }
}

struct McpDraft: Identifiable {
    enum Kind: String, CaseIterable, Identifiable {
        case command = "Command", url = "URL"
        var id: Self { self }
    }

    var existing: String?
    var name = ""
    var kind = Kind.command
    var command = ""
    var args = ""
    var url = ""
    var transport = "http"
    var env = ""
    var headers = ""
    var connections: Set<String> = []
    var id: String { existing ?? "+" }

    init() {}

    init(_ m: McpSpec) {
        existing = m.name
        name = m.name
        kind = m.url == nil ? .command : .url
        command = m.command ?? ""
        args = Lines.join(m.args)
        url = m.url ?? ""
        transport = m.transport ?? "http"
        env = Lines.join(m.env, separator: "=")
        headers = Lines.join(m.headers, separator: ": ")
        connections = Set(m.connections)
    }

    func spec() -> McpSpec? {
        guard let name = Lines.optional(name) else { return nil }
        switch kind {
        case .command:
            guard let command = Lines.optional(command) else { return nil }
            return McpSpec(name: name, command: command, args: Lines.split(args), env: Lines.pairs(env, separator: "="),
                           connections: connections.sorted())
        case .url:
            guard let url = Lines.optional(url) else { return nil }
            return McpSpec(name: name, url: url, transport: transport, headers: Lines.pairs(headers, separator: ":"),
                           connections: connections.sorted())
        }
    }
}

/// Adds or changes an MCP server: a command with args and env, or a url
/// with its type and headers, and the connections it needs.
private struct McpEditor: View {
    let workspace: SettingsWorkspace
    @State private var draft: McpDraft

    init(workspace: SettingsWorkspace, draft: McpDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing ?? "New MCP Server", ops: ops,
            remove: draft.existing.map { (.mcpRemove(workspace: workspace.name, name: $0), "Agents started later do not get it.") }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. docs", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("mcp.name")
                }
                Picker("Runs as", selection: $draft.kind) {
                    ForEach(McpDraft.Kind.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("mcp.kind")
            }
            switch draft.kind {
            case .command: command
            case .url: remote
            }
            if !workspace.connections.isEmpty { connections }
        }
    }

    private var command: some View {
        Section {
            TextField("Command, e.g. npx", text: $draft.command)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("mcp.command")
            TextField("Arguments, one per line", text: $draft.args, axis: .vertical)
                .lineLimit(1...6)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("mcp.args")
            TextField("Environment, NAME=value per line", text: $draft.env, axis: .vertical)
                .lineLimit(1...6)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("mcp.env")
        } footer: {
            Text("Plain values only: they are written to .muxa/mcp.json. Use ${VAR} of a connection for a secret.")
        }
    }

    private var remote: some View {
        Section {
            TextField("URL", text: $draft.url)
                .plainEntry(url: true)
                .accessibilityIdentifier("mcp.url")
            Picker("Type", selection: $draft.transport) {
                Text("http").tag("http")
                Text("sse").tag("sse")
            }
            .accessibilityIdentifier("mcp.type")
            TextField("Headers, Name: value per line", text: $draft.headers, axis: .vertical)
                .lineLimit(1...6)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("mcp.headers")
        } footer: {
            Text("Plain values only. A header may name a connection's variable as ${VAR}.")
        }
    }

    private var connections: some View {
        Section("Connections it needs") {
            ForEach(workspace.connections) { c in
                Toggle(c.name, isOn: Binding(
                    get: { draft.connections.contains(c.name) },
                    set: { on in
                        if on { draft.connections.insert(c.name) } else { draft.connections.remove(c.name) }
                    }))
                    .accessibilityIdentifier("mcp.connection.\(c.name)")
            }
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let spec = draft.spec() else { return nil }
        return draft.existing == nil
            ? [.mcpAdd(workspace: workspace.name, mcp: spec)]
            : [.mcpUpdate(workspace: workspace.name, mcp: spec)]
    }
}
