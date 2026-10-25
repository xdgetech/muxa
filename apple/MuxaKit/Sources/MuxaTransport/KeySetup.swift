@preconcurrency import Citadel
import Foundation
import NIOCore

/// One-time setup: logs in with the user's password and adds the device key to
/// `~/.ssh/authorized_keys`, restricted to the bridge. The password is used for this
/// one login and never stored.
public enum KeySetup {
    public enum Outcome: Equatable, Sendable {
        case added
        case alreadyPresent
    }

    /// The `authorized_keys` line for `key`: it may only run `command`, with no PTY
    /// and no forwarding (see SECURITY.md).
    public static func authorizedLine(for key: ClientKey, command: String, comment: String) throws -> String {
        // The command sits inside a quoted option; a quote or line break would end it.
        guard !command.isEmpty, !command.contains(where: { $0 == "\"" || $0.isNewline }) else {
            throw TransportError.failed("the bridge command cannot contain quotes or line breaks")
        }
        return "command=\"\(command)\",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding "
            + key.authorizedKey(comment: comment)
    }

    /// A POSIX shell script that appends `line` once. A new `~/.ssh` and file get
    /// owner-only permissions; an existing file keeps its own. Prints `added` or `present`.
    static func script(appending line: String) -> String {
        let quoted = "'" + line.replacingOccurrences(of: "'", with: "'\\''") + "'"
        return """
            umask 077 && mkdir -p "$HOME/.ssh" && f="$HOME/.ssh/authorized_keys" && touch "$f" && l=\(quoted) && \
            if grep -qxF -- "$l" "$f"; then echo present; else \
            { [ ! -s "$f" ] || [ -z "$(tail -c 1 "$f")" ] || echo; printf '%s\\n' "$l"; } >> "$f" && echo added; fi
            """
    }

    static func outcome(fromOutput output: String) throws -> Outcome {
        switch output.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "added": return .added
        case "present": return .alreadyPresent
        default: throw TransportError.failed("the host did not confirm the key was added")
        }
    }

    /// Logs in to `target` with `password` and adds `line`. The host key must already
    /// be pinned; otherwise this fails with `hostKeyMismatch` like `SSHBridge.connect`.
    public static func install(_ line: String, on target: SSHTarget, password: String) async throws -> Outcome {
        let check = HostKeyCheck(pinned: target.hostKey)
        let client: SSHClient
        do {
            client = try await SSHClient.connect(
                host: target.host,
                port: target.port,
                authenticationMethod: .passwordBased(username: target.user, password: password),
                hostKeyValidator: .custom(check),
                reconnect: .never,
                connectTimeout: .seconds(15))
        } catch {
            if let presented = check.rejected { throw TransportError.hostKeyMismatch(presented: presented) }
            if case SSHClientError.allAuthenticationOptionsFailed = error { throw TransportError.notAuthorized }
            throw TransportError.failed("cannot log in to \(target.host): \(error)")
        }
        do {
            let output = try await client.executeCommand(script(appending: line), maxResponseSize: 1024)
            try? await client.close()
            return try outcome(fromOutput: String(buffer: output))
        } catch let error as TransportError {
            try? await client.close()
            throw error
        } catch {
            try? await client.close()
            throw TransportError.failed("could not update authorized_keys on \(target.host): \(error)")
        }
    }
}
