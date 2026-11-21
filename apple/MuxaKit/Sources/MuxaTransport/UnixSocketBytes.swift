#if os(macOS)
import Foundation

/// A Unix socket as raw bytes: the Mac app on the host reaches the daemon's
/// `server.sock` without SSH (ADR 0070 decision 3).
public final class UnixSocketBytes: ByteChannel {
    public let bytes: AsyncThrowingStream<Data, any Error>
    private let handle: FileHandle

    /// `$XDG_STATE_HOME/muxa/server.sock`, else `~/.local/state/muxa/server.sock`,
    /// as the daemon places it.
    public static var serverSocket: URL {
        let env = ProcessInfo.processInfo.environment
        let state = env["XDG_STATE_HOME"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appending(path: ".local/state")
        return state.appending(path: "muxa/server.sock")
    }

    public init(path: String) throws {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw TransportError.failed("cannot open a socket: \(String(cString: strerror(errno)))") }
        // A write after the server closed fails with EPIPE instead of killing the app.
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let capacity = MemoryLayout.size(ofValue: addr.sun_path)
        guard path.utf8.count < capacity else {
            Darwin.close(fd)
            throw TransportError.failed("the socket path is too long: \(path)")
        }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: path.utf8)
            raw[path.utf8.count] = 0
        }
        let connected = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else {
            let why = String(cString: strerror(errno))
            Darwin.close(fd)
            throw TransportError.failed("cannot reach the muxa server at \(path): \(why). Is the daemon running?")
        }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        let (bytes, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        handle.readabilityHandler = { h in
            let chunk = h.availableData
            if chunk.isEmpty {
                h.readabilityHandler = nil
                out.finish()
            } else {
                out.yield(chunk)
            }
        }
        self.handle = handle
        self.bytes = bytes
    }

    public func send(_ data: Data) async throws {
        do {
            try handle.write(contentsOf: data)
        } catch {
            throw TransportError.closed
        }
    }

    public func close() async {
        handle.readabilityHandler = nil
        try? handle.close()
    }
}
#endif
