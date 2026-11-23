import Foundation

// A settings proposal's ops spell out their wire keys, as every settings type
// does, so the Inbox's snake-case decoder cannot read them. They decode apart
// with the plain decoder and go back on their items.

extension Inbox {
    /// `inbox` of a reply or notification, settings proposals included.
    static func field(of data: Data) throws -> Inbox {
        var inbox: Inbox = try MuxaV2.field("inbox", of: data)
        guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let raw = msg["inbox"] as? [String: Any] else { return inbox }
        inbox.items = withProposals(inbox.items, raw["items"])
        inbox.done = withProposals(inbox.done, raw["done"])
        return inbox
    }

    /// The Inbox as the wire spells it: what the demo server sends.
    var wireObject: Any {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        guard let data = try? e.encode(self),
              var object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return NSNull() }
        object["items"] = Self.wire(items, object["items"])
        object["done"] = Self.wire(done, object["done"])
        return object
    }

    private static func withProposals(_ items: [InboxItem], _ raw: Any?) -> [InboxItem] {
        let proposals = (raw as? [[String: Any]] ?? []).reduce(into: [String: Any]()) { out, item in
            if let id = item["id"] as? String, let p = item["settings_proposal"] { out[id] = p }
        }
        guard !proposals.isEmpty else { return items }
        return items.map { item in
            guard let raw = proposals[item.id] else { return item }
            var out = item
            // An op this client does not know (a newer host) leaves the item
            // without Apply; its note still lists the ops. The Inbox still shows.
            out.settingsProposal = (try? JSONSerialization.data(withJSONObject: raw))
                .flatMap { try? Coding.plainDecoder.decode(SettingsProposal.self, from: $0) }
            return out
        }
    }

    private static func wire(_ items: [InboxItem], _ encoded: Any?) -> Any? {
        guard var list = encoded as? [[String: Any]] else { return encoded }
        for (i, item) in items.enumerated() where i < list.count {
            guard let p = item.settingsProposal, let data = try? Coding.plainEncoder.encode(p) else { continue }
            list[i]["settings_proposal"] = try? JSONSerialization.jsonObject(with: data)
        }
        return list
    }
}
