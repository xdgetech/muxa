import MuxaV2
import SwiftUI

/// The build disk (ADR 0102): `[build]` in `workspaces.toml`. Shown only
/// when the host sends it; an older host sends none.
struct BuildSettings: View {
    @Environment(NextModel.self) private var model
    @State private var dir = ""
    @State private var dropAfter = ""
    @State private var extraPaths: [EditableLine] = []
    @State private var error: String?

    var body: some View {
        let build = model.settings.settings?.build ?? SettingsBuild()
        SettingsPage {
            Section {
                field("Folder", text: $dir, prompt: "/Volumes/Build/muxa", saved: build.dir, id: "build.dir") {
                    .buildUpdate(dir: change(dir))
                }
                status(build)
            } header: {
                Text("Folder")
            } footer: {
                Text("A folder on the host. ~/ is the home folder. Empty turns the build disk off.")
            }
            Section {
                field("Drop build output after", text: $dropAfter, prompt: "3d", saved: build.dropAfter, id: "build.dropafter") {
                    .buildUpdate(dropAfter: change(dropAfter))
                }
            } header: {
                Text("Drop build output after")
            } footer: {
                Text("Each Task worktree's build output goes to its own folder here. "
                    + "After an ended Task waits this long, its build output is removed; its work stays.")
            }
            extra(build)
            SaveError(text: error, id: "build.error")
        }
        .navigationTitle("Build disk")
        .inlineTitle()
        .task(id: model.settings.settings?.revision) { show(model.settings.settings?.build) }
    }

    private func field(_ title: String, text: Binding<String>, prompt: String, saved: String?, id: String,
                       op: @escaping () -> SettingsOp) -> some View {
        HStack {
            TextField(title, text: text, prompt: Text(prompt))
                .labelsHidden()
                .plainEntry()
                .accessibilityIdentifier(id)
            if Lines.optional(text.wrappedValue) != saved {
                Button("Save") { Task { error = await model.settings.save([op()]) } }
                    .disabled(model.settings.saving || model.settings.readOnly)
                    .accessibilityIdentifier("\(id).save")
            }
        }
    }

    /// Folders muxa cannot find from a build file (ADR 0102).
    private func extra(_ build: SettingsBuild) -> some View {
        let paths = extraPaths.map { Lines.trimmed($0.text) }.filter { !$0.isEmpty }
        return Section {
            EditableLineList(lines: $extraPaths, placeholder: "out", addTitle: "Add a path", id: "build.extra",
                             problem: Self.pathProblem)
            Button("Save") { Task { error = await model.settings.save([.buildUpdate(extraPaths: paths)]) } }
                .disabled(paths == build.extraPaths || extraPaths.contains { Self.pathProblem($0.text) != nil }
                    || model.settings.saving || model.settings.readOnly)
                .accessibilityIdentifier("build.extra.save")
        } header: {
            Text("Extra paths")
        } footer: {
            Text("muxa finds each repo's build folders from its build files: target for Cargo, .build for SwiftPM, "
                + "build and .gradle for Gradle, .next for Next.js. Add any other folder here. "
                + "A path is used only where the repo's git ignores it.")
        }
    }

    /// The host's rule, said early: a relative path of plain names.
    static func pathProblem(_ text: String) -> String? {
        let t = Lines.trimmed(text)
        let plain = !t.hasPrefix("/") && !t.hasPrefix("~")
            && !t.split(separator: "/").contains { $0 == "." || $0 == ".." }
        return plain ? nil : "A path inside the repo, like out or web/dist."
    }

    @ViewBuilder
    private func status(_ build: SettingsBuild) -> some View {
        if build.dir != nil {
            if build.dirFound {
                Label("The host finds this folder.", systemImage: "checkmark.circle")
                    .font(.callout)
                    .foregroundStyle(Theme.connected)
                    .accessibilityIdentifier("build.status")
            } else {
                Label("The host cannot find this folder. Is the disk mounted? Tasks build in their worktree until it is.",
                      systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(Theme.attention)
                    .accessibilityIdentifier("build.status")
            }
        }
    }

    /// An empty field removes the setting.
    private func change(_ text: String) -> FieldChange<String> {
        Lines.optional(text).map(FieldChange.set) ?? .clear
    }

    private func show(_ build: SettingsBuild?) {
        dir = build?.dir ?? ""
        dropAfter = build?.dropAfter ?? ""
        extraPaths = (build?.extraPaths ?? []).map(EditableLine.init)
    }
}
