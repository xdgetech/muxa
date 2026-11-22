#if os(macOS)
import Foundation

/// A local process as raw bytes, run with `/bin/sh -c`. For tests and tools.
public final class ProcessBytes: ByteChannel {
    public let bytes: AsyncThrowingStream<Data, any Error>
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
        let (bytes, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        output.fileHandleForReading.readabilityHandler = { handle in
            let chunk = handle.availableData
            if chunk.isEmpty {
                handle.readabilityHandler = nil
                out.finish()
            } else {
                out.yield(chunk)
            }
        }
        try process.run()
        self.process = process
        self.stdin = input.fileHandleForWriting
        self.bytes = bytes
    }

    public func send(_ data: Data) async throws {
        do {
            try stdin.write(contentsOf: data)
        } catch {
            throw TransportError.closed
        }
    }

    public func close() async {
        try? stdin.close()
        if process.isRunning { process.terminate() }
    }
}
#endif
