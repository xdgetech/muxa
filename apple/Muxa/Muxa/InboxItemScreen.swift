import MuxaProtocol
import MuxaSession
import SwiftUI

/// One Inbox item (docs/inbox.md, "The item screen"): the question and its source,
/// the summary, the evidence, a reply box and the answers. Opening it marks it read;
/// it never answers it. `answered` gets the item as the host saved it.
struct InboxItemScreen: View {
    @Environment(AppModel.self) private var model
    let id: String
    /// Pushed on a phone, it has its own title; beside the list it keeps the Inbox's.
    var pushed = true
    var answered: (InboxItem) -> Void
    @State private var reply = ""
    @State private var later: InboxItem?
    @State private var approving: InboxItem?
    @State private var sending = false
    @State private var failure: String?
    @FocusState private var replying: Bool

    var body: some View {
        if let item = model.item(id) {
            content(item)
                .task(id: item.id) { await model.session.markRead(item) }
        } else {
            ContentUnavailableView("Item Not Found", systemImage: "tray",
                                   description: Text("Another device or the terminal answered it."))
        }
    }

    private func content(_ item: InboxItem) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.question).font(.title3.weight(.semibold))
                        .accessibilityIdentifier("item-question")
                    Text(source(item)).font(.subheadline).foregroundStyle(.secondary)
                }
                if let summary = item.summary, !summary.isEmpty {
                    Text(summary).textSelection(.enabled).accessibilityIdentifier("item-summary")
                }
                if let question = item.prompt?.question {
                    Text(question).font(.headline)
                }
                if item.kind == .proposal, let p = item.proposal {
                    VStack(alignment: .leading, spacing: 6) {
                        LabeledContent("Action", value: p.action)
                        LabeledContent("Target", value: p.target)
                        Text(p.content).font(.callout.monospaced()).textSelection(.enabled)
                    }
                    .padding(12)
                    .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
                }
                ItemEvidence(item: item)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .scrollDismissesKeyboard(.interactively)
        // In the bottom inset, the reply and the answers stay above the keyboard.
        .safeAreaInset(edge: .bottom) { answerBar(item) }
        .navigationTitle(pushed ? item.kind.title : "Inbox")
        .navigationBarTitleDisplayMode(.inline)
        .itemLater($later, saved: answered)
        .sheet(item: $approving) { item in
            ApproveSheet(title: item.title, workspace: item.workspace, proposal: item.proposal) {
                approving = nil
                send(item, .approve)
            }
        }
        .alert("Can't Answer", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        .background {
            let main = ItemAnswers(item).main
            Button("Send the Main Answer") { if let main { perform(item, main) } }
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(main == nil || sending)
                .hidden()
        }
    }

    private func source(_ item: InboxItem) -> String {
        let from = item.routine.map { "\(item.workspace)/\($0)" } ?? "\(item.title) · \(item.workspace)"
        return "\(from) · \(item.age())"
    }

    // MARK: Answers

    private func answerBar(_ item: InboxItem) -> some View {
        let answers = ItemAnswers(item)
        let terminal = model.itemSession(item)
        return VStack(spacing: 8) {
            if item.answers.contains(where: \.needsText) {
                TextField("Reply…", text: $reply, axis: .vertical)
                    .lineLimit(1...4)
                    .textFieldStyle(.roundedBorder)
                    .focused($replying)
                    .accessibilityIdentifier("item-reply")
            }
            if answers.chooses, let options = item.prompt?.options {
                ViewThatFits {
                    HStack { optionButtons(item, options) }
                    VStack(alignment: .leading) { optionButtons(item, options) }
                }
            }
            HStack {
                ForEach(answers.shown, id: \.self) { action in
                    answerButton(item, action, prominent: action == answers.main)
                }
                Spacer(minLength: 0)
                if !answers.more.isEmpty || terminal != nil {
                    Menu {
                        ForEach(answers.more, id: \.self) { action in
                            Button(action.title(for: item.kind)) { perform(item, action) }
                                .disabled(action.needsText && !hasReply)
                                .accessibilityIdentifier("item-answer-\(action.rawValue)")
                        }
                        if let terminal {
                            Button("Open Terminal", systemImage: Icon.agent) { model.openTerminal(terminal) }
                                .accessibilityIdentifier("item-open-terminal")
                        }
                    } label: {
                        Label("More", systemImage: "ellipsis.circle")
                    }
                    .accessibilityIdentifier("item-more")
                }
            }
            .disabled(sending)
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(.bar)
    }

    @ViewBuilder
    private func optionButtons(_ item: InboxItem, _ options: [ChatPrompt.Option]) -> some View {
        ForEach(Array(options.enumerated()), id: \.offset) { index, option in
            Button(option.label) { send(item, .choose, option: UInt32(index)) }
                .buttonStyle(.bordered)
                .tint(option.cancel ? .secondary : .accentColor)
                .disabled(sending)
                .accessibilityIdentifier("item-option-\(index)")
        }
    }

    @ViewBuilder
    private func answerButton(_ item: InboxItem, _ action: InboxAction, prominent: Bool) -> some View {
        let button = Button(action.title(for: item.kind)) { perform(item, action) }
            .disabled(action.needsText && !hasReply)
            .accessibilityIdentifier("item-answer-\(action.rawValue)")
        if prominent {
            button.buttonStyle(.borderedProminent)
        } else {
            button.buttonStyle(.bordered)
        }
    }

    private var hasReply: Bool { !reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// Later asks for a time, and Approve on a proposal shows what the Task will get.
    private func perform(_ item: InboxItem, _ action: InboxAction) {
        switch action {
        case .later: later = item
        case .approve where item.kind == .proposal: approving = item
        case _ where action.needsText && !hasReply: replying = true
        default: send(item, action)
        }
    }

    private func send(_ item: InboxItem, _ action: InboxAction, option: UInt32? = nil) {
        let text = action.needsText ? reply.trimmingCharacters(in: .whitespacesAndNewlines) : nil
        sending = true
        Task {
            defer { sending = false }
            do {
                let saved = try await model.session.answer(item, action, text: text, option: option)
                reply = ""
                replying = false
                answered(saved)
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }
}

/// The item's evidence as chips. Changes, docs and images open in the Reader for
/// the item's pane and return to the item; links open in the browser.
private struct ItemEvidence: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem

    var body: some View {
        let a = item.attachments
        if !a.isEmpty {
            let readable = model.canOpenEvidence(item)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack {
                    if let c = a.changes {
                        chip("Changes +\(c.added) −\(c.removed)", Icon.changes, id: "changes") {
                            model.openEvidence(item, as: .changes)
                        }
                        .disabled(!readable || !model.session.canShowChanges)
                    }
                    ForEach(a.docs, id: \.self) { path in
                        chip(Self.name(path), Icon.document, id: "doc-\(path)") { model.openEvidence(item, as: .doc(path)) }
                            .disabled(!readable)
                    }
                    ForEach(a.images, id: \.self) { path in
                        chip(Self.name(path), "photo", id: "image-\(path)") { model.openEvidence(item, as: .image(path)) }
                            .disabled(!readable || !model.session.canShowImages)
                    }
                    ForEach(a.links, id: \.url) { link in
                        if let url = URL(string: link.url) {
                            Link(destination: url) { Label(link.title ?? link.url, systemImage: "link") }
                                .buttonStyle(.bordered)
                                .buttonBorderShape(.capsule)
                                .accessibilityIdentifier("item-evidence-link")
                        }
                    }
                }
            }
            .accessibilityIdentifier("item-evidence")
        }
    }

    private func chip(_ title: String, _ icon: String, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Label(title, systemImage: icon) }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .accessibilityIdentifier("item-evidence-\(id)")
    }

    private static func name(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }
}

/// Later for an item: in 1 hour, tomorrow 9:00, or a time the user picks.
private struct ItemLaterModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var item: InboxItem?
    var saved: (InboxItem) -> Void
    @State private var picking: InboxItem?
    @State private var failure: String?

    func body(content: Content) -> some View {
        content
            .confirmationDialog(
                "Show Again",
                isPresented: Binding(get: { item != nil }, set: { if !$0 { item = nil } }),
                titleVisibility: .visible,
                presenting: item
            ) { item in
                ForEach(LaterChoice.allCases, id: \.title) { choice in
                    Button(choice.title) { save(item, until: choice.until()) }
                }
                Button("Pick a Time…") { DispatchQueue.main.async { picking = item } }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("It leaves the Inbox until then, on every device.")
            }
            .sheet(item: $picking) { item in
                LaterPicker { until in
                    picking = nil
                    save(item, until: until)
                }
            }
            .alert("Can't Update the Inbox", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
    }

    private func save(_ item: InboxItem, until: UInt64) {
        Task {
            do {
                saved(try await model.session.answer(item, .later, until: until))
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }
}

extension View {
    func itemLater(_ item: Binding<InboxItem?>, saved: @escaping (InboxItem) -> Void = { _ in }) -> some View {
        modifier(ItemLaterModifier(item: item, saved: saved))
    }
}
