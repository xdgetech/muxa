import MuxaV2
import SwiftUI

/// The Task Loop of a Task's agent (ADR 0019, plan F3): where it stands, and
/// Start, Start Again and Stop. The host runs the check after each finished
/// turn and types the fix prompt; muxa never answers the agent's menus.
struct LoopSheet: View {
    @Environment(NextModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let terminal: MuxaV2.Terminal
    @State private var check = ""
    @State private var attempts = 3

    var body: some View {
        NavigationStack {
            PaletteForm {
                if let lp = model.loops[terminal.id] {
                    Section("Now") {
                        LabeledContent("State", value: LoopText.state(lp))
                        LabeledContent("Failed checks", value: "\(lp.attempts) of \(lp.maxAttempts)")
                        LabeledContent("Check") { Text(lp.check).monospaced().lineLimit(2) }
                        if let last = lp.lastCheck {
                            LabeledContent("Last check", value: LoopText.last(last))
                        }
                    }
                    .accessibilityIdentifier("loop.now")
                }
                Section {
                    TextField("The workspace's check", text: $check)
                        .monospaced()
                        .plainEntry()
                        .accessibilityIdentifier("loop.check")
                    Stepper("Stop after \(attempts) failed checks", value: $attempts, in: 1...10)
                } header: {
                    Text("Check")
                } footer: {
                    Text("After each finished turn the host runs this in the Task's folder. "
                        + "Leave it empty for the workspace's only allowed check.")
                }
                if let error = model.requestError {
                    Text(error).foregroundStyle(Theme.failure).accessibilityIdentifier("loop.error")
                }
            }
            .navigationTitle("Task Loop")
            .inlineTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(model.loops[terminal.id] == nil ? "Start" : "Start Again") { Task { await start() } }
                        .accessibilityIdentifier("loop.start")
                }
                if model.loops[terminal.id]?.isActive == true {
                    ToolbarItem(placement: .destructiveAction) {
                        Button("Stop", role: .destructive) { Task { await model.stopLoop(terminal.id) } }
                            .accessibilityIdentifier("loop.stop")
                    }
                }
            }
            .onAppear {
                if let lp = model.loops[terminal.id] {
                    check = lp.check
                    attempts = Int(lp.maxAttempts)
                }
            }
        }
    }

    private func start() async {
        let trimmed = check.trimmingCharacters(in: .whitespaces)
        let spec = LoopSpec(check: trimmed.isEmpty ? nil : trimmed, maxAttempts: UInt32(attempts))
        await model.startLoop(terminal.id, spec: spec, reset: true)
        if model.requestError == nil { dismiss() }
    }
}

/// How a loop reads in a row or a sheet.
enum LoopText {
    static func state(_ lp: LoopInfo) -> String {
        switch (lp.state, lp.reason) {
        case ("armed", _): "Waits for the turn's end"
        case ("checking", _): "Checking"
        case ("paused", _): "Paused: the agent waits for you"
        case ("passed", _): "Passed"
        case ("stopped", "limit"): "Stopped: too many failed checks"
        case ("stopped", "user"): "Stopped"
        case ("stopped", "no-agent"): "Stopped: the agent ended"
        case ("stopped", let r?): "Stopped: \(r)"
        default: lp.state
        }
    }

    /// The Loop button: `Loop`, or `Loop 1/3` while one runs.
    static func title(_ lp: LoopInfo?) -> String {
        guard let lp, lp.isActive else { return "Loop" }
        return "Loop \(lp.attempts)/\(lp.maxAttempts)"
    }

    /// `loop 1/3`, short for a list row.
    static func badge(_ lp: LoopInfo) -> String {
        "loop \(lp.state) \(lp.attempts)/\(lp.maxAttempts)"
    }

    static func last(_ c: LastCheck) -> String {
        let how = c.exit.map { $0 == 0 ? "passed" : "failed with \($0)" } ?? "timed out"
        return "#\(c.attempt) \(how) in \(c.seconds) s"
    }
}
