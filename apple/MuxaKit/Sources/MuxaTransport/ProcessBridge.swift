#if os(macOS)
import Foundation

/// The bridge as a local process, run with `/bin/sh -c`. For tests and tools on the host.
public final class ProcessBridge: BridgeChannel {
    public let lines: AsyncThrowingStream<Data, any Error>
    private let process: Process
    private let stdin: FileHandle

    public init(command: String) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", command]
        let input = Pipe()
        let output = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        let (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        let framer = Locked(LineFramer())
        output.fileHandleForReading.readabilityHandler = { handle in
            let chunk = handle.availableData
            if chunk.isEmpty {
                handle.readabilityHandler = nil
                out.finish()
                return
            }
            do {
                for line in try framer.withValue({ try $0.push(chunk) }) { out.yield(line) }
            } catch {
                handle.readabilityHandler = nil
                out.finish(throwing: error)
            }
        }
        try process.run()
        self.process = process
        self.stdin = input.fileHandleForWriting
        self.lines = lines
    }

    public func send(_ line: Data) async throws {
        do {
            try stdin.write(contentsOf: line + Data([UInt8(ascii: "\n")]))
        } catch {
            throw TransportError.closed
        }
    }

    public func close() async {
        try? stdin.close()
        if process.isRunning { process.terminate() }
    }
}

final class Locked<Value>: @unchecked Sendable {
    private var value: Value
    private let lock = NSLock()

    init(_ value: Value) { self.value = value }

    func withValue<T>(_ body: (inout Value) throws -> T) rethrows -> T {
        try lock.withLock { try body(&value) }
    }
}
#endif
