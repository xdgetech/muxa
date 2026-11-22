import MuxaV2
import SwiftUI

/// The sidebar's Workspaces: each one folds, lists its repos on their branch
/// with the agent of the repo's checkout (open or start), the Tasks in each
/// repo, then the Tasks with no repo.
struct FocusWorkspaces: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let groups: [WorkspaceGroup]
    @Binding var selected: String?
    /// Folded Workspaces by name.
    @State private var folded: Set<String> = []
    /// Repos whose checkout agent is starting, as `<ws>.<repo>`.
    @State private var starting: Set<String> = []

    var body: some View {
        HStack {
            SectionLabel(title: "Workspaces")
            Spacer()
            Button { nav.newTask = TaskSeed(workspace: model.workspaceList.first?.name) } label: {
                Image(systemName: "plus").font(.system(size: 13, weight: .medium))
            }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.textSecondary)
            .help("New Task (\(Shortcut.newTask.display))")
            .accessibilityLabel("New Task")
            .accessibilityIdentifier("sidebar.newtask")
        }
        .padding(.horizontal, 10)
        .padding(.top, 14)
        .padding(.bottom, 4)
        ForEach(groups) { group in
            VStack(alignment: .leading, spacing: 1) {
                header(group)
                if !folded.contains(group.id) { content(group) }
            }
            .padding(.bottom, 6)
        }
    }

    private func header(_ g: WorkspaceGroup) -> some View {
        let open = !folded.contains(g.id)
        return Button {
            if open { folded.insert(g.id) } else { folded.remove(g.id) }
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Palette.textTertiary)
                    .rotationEffect(.degrees(open ? 0 : -90))
                Text(g.workspace.name).font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.text)
                Spacer()
                Text("\(g.count)").font(.system(size: 11)).foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityValue(open ? "open" : "closed")
        .accessibilityIdentifier("workspace.\(g.id)")
    }

    @ViewBuilder private func content(_ g: WorkspaceGroup) -> some View {
        ForEach(g.repos) { repo in
            repoRow(repo, in: g.workspace)
            ForEach(repo.tasks) { task($0, style: .task) }
        }
        if !g.loose.isEmpty {
            HStack(spacing: 8) {
                Image(systemName: "folder").font(.system(size: 11))
                Text("No repo · reads all repos").font(.system(size: 12))
            }
            .foregroundStyle(Palette.textTertiary)
            .padding(.leading, 26)
            .padding(.vertical, 3)
            ForEach(g.loose) { task($0, style: .loose) }
        }
    }

    private func repoRow(_ repo: WorkspaceGroup.Repo, in ws: Workspace) -> some View {
        let key = "\(ws.name).\(repo.name)"
        return HStack(spacing: 8) {
            Image(systemName: "arrow.triangle.branch")
                .font(.system(size: 11))
                .foregroundStyle(Palette.textTertiary)
            Text(repo.name)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Palette.textMid)
                .lineLimit(1)
                .truncationMode(.tail)
            if let branch = repo.branch {
                Text(branch)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(1)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(Palette.pill, in: RoundedRectangle(cornerRadius: 4))
            }
            Spacer(minLength: 4)
            if let agent = repo.checkout {
                OpenPill { nav.open(agent) }.accessibilityIdentifier("repo.open.\(key)")
            } else {
                StartButton(busy: starting.contains(key)) { start(repo.name, in: ws, key: key) }
                    .accessibilityIdentifier("repo.start.\(key)")
            }
        }
        .padding(.leading, 26)
        .padding(.trailing, 6)
        .padding(.vertical, 3)
        .selectableRow(repo.checkout.map { $0.id == selected } ?? false)
        .accessibilityElement(children: .contain)
        .accessibilityLabel([repo.name, repo.branch].compactMap { $0 }.joined(separator: " "))
        .accessibilityIdentifier("repo.\(key)")
    }

    private func task(_ t: MuxaV2.Terminal, style: FocusTaskRow.Style) -> some View {
        FocusTaskRow(terminal: t, style: style, loop: model.loops[t.id], indent: 36, selected: t.id == selected) {
            selected = t.id
        }
    }

    private func start(_ repo: String, in ws: Workspace, key: String) {
        starting.insert(key)
        Task {
            let agent = await model.startCheckout(workspace: ws, repo: repo)
            starting.remove(key)
            if let agent { nav.open(agent) }
        }
    }
}

/// The green "open" pill of a repo whose checkout agent runs.
private struct OpenPill: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Circle().fill(TaskState.working.color).frame(width: 6, height: 6)
                Text("open")
            }
            .font(.system(size: 11))
            .foregroundStyle(Palette.okText)
            .padding(.horizontal, 7)
            .frame(height: 22)
            .background(Palette.okBack, in: RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Open the checkout's agent")
    }
}

/// The outlined "▶ start" button of a repo with no checkout agent.
private struct StartButton: View {
    let busy: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "play.fill").font(.system(size: 8))
                Text("start")
            }
            .font(.system(size: 11))
            .foregroundStyle(Palette.textSecondary)
            .padding(.horizontal, 7)
            .frame(height: 22)
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .accessibilityLabel("Start an agent in the checkout")
    }
}
