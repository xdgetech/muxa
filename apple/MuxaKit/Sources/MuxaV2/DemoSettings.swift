import Foundation

/// Why the demo refuses an op; the message never holds a secret or a value.
struct DemoSettingsRefusal: Error {
    var message: String
}

extension Settings {
    /// The settings after `ops`, as the server applies them without its file
    /// checks: all or none, and the error names the op that failed. A secret
    /// only makes the connection's secret `set`; an env value is dropped.
    func demoApplying(_ raw: [Any]) throws -> Settings {
        var out = self
        for (i, value) in raw.enumerated() {
            let name = (value as? [String: Any])?["op"] as? String ?? "?"
            do {
                let data = try JSONSerialization.data(withJSONObject: value)
                let op = try Coding.plainDecoder.decode(SettingsOp.self, from: data)
                try out.demoApply(op)
            } catch let refusal as DemoSettingsRefusal {
                throw DemoSettingsRefusal(message: "ops[\(i)] \(name): \(refusal.message)")
            } catch {
                throw DemoSettingsRefusal(message: "ops[\(i)] \(name): not a known op with its fields")
            }
        }
        out.revision = String((Int(revision) ?? 0) + 1)
        return out
    }

    private mutating func demoApply(_ op: SettingsOp) throws {
        switch op {
        case let .workspaceAdd(name, root, repos):
            guard !workspaces.contains(where: { $0.name == name }) else { throw refusal("\(name) exists") }
            workspaces.append(SettingsWorkspace(name: name, root: root, repos: repos))
        case let .workspaceUpdate(workspace, root, repos, agentCommand, parkAfter, hiddenRepos, claudeConnectors, botRules,
                                  buildPaths):
            try edit(workspace) { w in
                if let buildPaths { w.buildPaths = buildPaths }
                if let botRules { w.botRules = botRules }
                if let claudeConnectors { w.claudeConnectors = claudeConnectors }
                if let root { w.root = root }
                if let repos { w.repos = repos }
                if let hiddenRepos { w.hiddenRepos = hiddenRepos }
                w.agent.command = agentCommand.applied(to: w.agent.command)
                w.parkAfter = parkAfter.applied(to: w.parkAfter)
            }
        case let .workspaceRemove(workspace):
            _ = try index(workspace)
            workspaces.removeAll { $0.name == workspace }
        case let .envSet(workspace, key, _):
            try edit(workspace) { w in
                guard !w.agent.env.contains(where: { $0.key == key }) else { return }
                w.agent.env.append(EnvKey(key: key))
                w.agent.env.sort { $0.key < $1.key }
            }
        case let .envUnset(workspace, key):
            try edit(workspace) { w in
                guard w.agent.env.contains(where: { $0.key == key }) else { throw refusal("no variable \(key)") }
                w.agent.env.removeAll { $0.key == key }
            }
        case let .loopChecks(workspace, checks):
            try edit(workspace) { $0.loop = checks.map(WorkspaceLoop.init) }
        case let .routinesGate(workspace, enabled, directWrites):
            try edit(workspace) { w in
                w.routinesEnabled = enabled
                w.directWrites = directWrites
            }
        case let .connectionAdd(workspace, spec, secret):
            try edit(workspace) { w in
                guard !w.connections.contains(where: { $0.name == spec.name }) else { throw refusal("\(spec.name) exists") }
                w.connections.append(SettingsConnection(spec: spec, secret: secret == nil ? .missing : .set))
            }
        case let .connectionUpdate(workspace, spec, secret):
            try edit(workspace) { w in
                let at = try find(w.connections, spec.name, \.name)
                w.connections[at] = SettingsConnection(spec: spec, secret: secret == nil ? w.connections[at].secret : .set)
            }
        case let .connectionSecret(workspace, name, _):
            try edit(workspace) { w in w.connections[try find(w.connections, name, \.name)].secret = .set }
        case let .connectionRemove(workspace, name):
            try edit(workspace) { w in w.connections.remove(at: try find(w.connections, name, \.name)) }
        case let .mcpAdd(workspace, mcp):
            try edit(workspace) { w in
                guard !w.mcp.contains(where: { $0.name == mcp.name }) else { throw refusal("\(mcp.name) exists") }
                w.mcp.append(mcp)
            }
        case let .mcpUpdate(workspace, mcp):
            try edit(workspace) { w in w.mcp[try find(w.mcp, mcp.name, \.name)] = mcp }
        case let .mcpRemove(workspace, name):
            try edit(workspace) { w in w.mcp.remove(at: try find(w.mcp, name, \.name)) }
        case let .routineAdd(workspace, spec, text):
            try checkTiming(spec)
            try edit(workspace) { w in
                guard !w.routines.contains(where: { $0.name == spec.name }) else { throw refusal("\(spec.name) exists") }
                w.routines.append(SettingsRoutine(spec: spec, instructionsText: text))
            }
        case let .routineUpdate(workspace, spec, text):
            try checkTiming(spec)
            try edit(workspace) { w in
                let at = try find(w.routines, spec.name, \.name)
                w.routines[at] = SettingsRoutine(spec: spec, instructionsText: text ?? w.routines[at].instructionsText)
            }
        case let .routineRemove(workspace, name):
            try edit(workspace) { w in w.routines.remove(at: try find(w.routines, name, \.name)) }
        case let .botAdd(workspace, spec, text):
            try edit(workspace) { w in
                guard !w.bots.contains(where: { $0.name == spec.name }) else { throw refusal("\(spec.name) exists") }
                try checkTeam(spec, in: w)
                w.bots.append(SettingsBot(spec: spec, soulText: text ?? ""))
            }
        case let .botUpdate(workspace, spec, text):
            try edit(workspace) { w in
                let at = try find(w.bots, spec.name, \.name)
                try checkTeam(spec, in: w)
                w.bots[at].spec = spec
                if let text { w.bots[at].soulText = text }
            }
        case let .botRemove(workspace, name):
            try edit(workspace) { w in
                guard !w.routines.contains(where: { $0.spec.bot == name }) else {
                    throw refusal("\(name) still owns Routines; remove them first")
                }
                w.bots.remove(at: try find(w.bots, name, \.name))
            }
        case let .botRename(workspace, from, to):
            try edit(workspace) { w in
                let at = try find(w.bots, from, \.name)
                guard BotSpec.isValidName(to) else { throw refusal("\(to) is not a Bot name") }
                guard !w.bots.contains(where: { $0.name == to }) else { throw refusal("\(to) exists") }
                w.bots[at].spec.name = to
                for i in w.bots.indices {
                    w.bots[i].spec.members = w.bots[i].spec.members.map { $0 == from ? to : $0 }
                }
                for i in w.routines.indices where w.routines[i].spec.bot == from { w.routines[i].spec.bot = to }
            }
        case let .botMemoryRestore(workspace, name, version):
            try edit(workspace) { w in
                let at = try find(w.bots, name, \.name)
                guard version >= 1, version <= w.bots[at].memoryVersions else { throw refusal("no kept memory \(version)") }
            }
        case let .assistantUpdate(enabled, worker, text):
            let old = assistant ?? SettingsAssistant()
            assistant = SettingsAssistant(enabled: enabled, worker: worker, soulText: text ?? old.soulText,
                                          memoryText: old.memoryText)
        case let .buildUpdate(dir, dropAfter):
            let old = build ?? SettingsBuild()
            let newDir = dir.applied(to: old.dir)
            // The demo host has every folder it is given.
            build = SettingsBuild(dir: newDir, dropAfter: dropAfter.applied(to: old.dropAfter), dirFound: newDir != nil)
        case let .workerAdd(worker):
            guard !workers.contains(where: { $0.name == worker.name }) else { throw refusal("\(worker.name) exists") }
            workers.append(worker)
        case let .workerUpdate(worker):
            workers[try find(workers, worker.name, \.name)] = worker
        case let .workerRemove(name):
            workers.remove(at: try find(workers, name, \.name))
        }
    }

    private func index(_ workspace: String) throws -> Int {
        guard let at = workspaces.firstIndex(where: { $0.name == workspace }) else { throw refusal("no Workspace \(workspace)") }
        return at
    }

    private mutating func edit(
        _ workspace: String, _ f: (inout SettingsWorkspace) throws -> Void
    ) throws {
        let at = try index(workspace)
        try f(&workspaces[at])
    }
}

/// `on` is only `review`, and such a Routine has no time (ADR 0084).
private func checkTiming(_ r: RoutineSpec) throws {
    guard let on = r.on else { return }
    guard on == "review" else { throw refusal("on is review or absent") }
    guard r.at == nil, r.schedule == nil else { throw refusal("a Routine with on has no at or schedule") }
}

/// A lead's members are other Bots of the Workspace that lead no team (ADR 0085).
private func checkTeam(_ b: BotSpec, in w: SettingsWorkspace) throws {
    guard !b.members.isEmpty else { return }
    guard b.members.count <= 6 else { throw refusal("a team has at most 6 members") }
    if let lead = w.bots.first(where: { $0.name != b.name && $0.spec.members.contains(b.name) }) {
        throw refusal("\(b.name) is a member of \(lead.name) and cannot lead a team")
    }
    for m in b.members {
        guard m != b.name, let member = w.bots.first(where: { $0.name == m }) else { throw refusal("no other Bot \(m)") }
        guard member.spec.members.isEmpty else { throw refusal("\(m) leads a team itself") }
    }
    if let handoffs = b.handoffsPerDay, !(1...50).contains(handoffs) { throw refusal("handoffs_per_day is 1 to 50") }
}

private func find<T>(_ list: [T], _ name: String, _ key: KeyPath<T, String>) throws -> Int {
    guard let at = list.firstIndex(where: { $0[keyPath: key] == name }) else { throw refusal("no \(name)") }
    return at
}

private func refusal(_ message: String) -> DemoSettingsRefusal { DemoSettingsRefusal(message: message) }

extension FieldChange {
    func applied(to value: Value?) -> Value? {
        switch self {
        case .keep: value
        case .clear: nil
        case let .set(new): new
        }
    }
}
