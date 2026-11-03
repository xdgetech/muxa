/// Explicit recovery never retries a process replacement or prompt delivery.
public enum RestartRecoveryAction: String, Codable, CaseIterable, Equatable, Sendable {
    case cancelPrepared = "cancel-prepared"
    case finishReplacement = "finish-replacement"
    case leaveUnconfirmed = "leave-unconfirmed"
}

/// Public recovery metadata only. Unknown phases/actions remain non-actionable.
public struct RestartOperation: Codable, Equatable, Sendable {
    public var id: String
    public var phase: String
    /// The requested worker name or `default`, not a confirmed native role.
    public var worker: String?
    public var actions: [String]

    private enum CodingKeys: String, CodingKey { case id, phase, worker, actions }
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        phase = try c.decode(String.self, forKey: .phase)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        actions = try c.decodeIfPresent([String].self, forKey: .actions) ?? []
    }
    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(phase, forKey: .phase)
        try c.encodeIfPresent(worker, forKey: .worker)
        if !actions.isEmpty { try c.encode(actions, forKey: .actions) }
    }

    public init(id: String, phase: String, worker: String? = nil, actions: [String] = []) {
        self.id = id
        self.phase = phase
        self.worker = worker
        self.actions = actions
    }
}
