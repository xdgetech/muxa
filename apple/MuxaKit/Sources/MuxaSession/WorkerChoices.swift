import MuxaProtocol

extension WorkerInfo {
    public func allows(workspace: String) -> Bool {
        workspaces?.contains(workspace) ?? true
    }
}

/// A run's or operation's requested worker. `nil` is a legacy run: unrecorded.
public func workerLabel(_ name: String?) -> String {
    guard let name else { return "Worker unrecorded" }
    return name == defaultWorker ? "Workspace default" : name
}

/// An open sheet may retain a selection after the host changes its definitions.
public func workerProblem(name: String?, workspace: String, workers: [WorkerInfo]) -> String? {
    guard let name, name != defaultWorker else { return nil }
    return workers.contains { $0.name == name && $0.allows(workspace: workspace) } ? nil
        : "This worker is no longer available in this workspace. Pick a worker again."
}
