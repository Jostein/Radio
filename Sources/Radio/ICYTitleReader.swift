import Foundation

/// Reads `StreamTitle` from an Icecast/Shoutcast stream's inline metadata.
///
/// Metadata sits *inside* the audio stream at every `icy-metaint` bytes, so
/// reaching it means reading (and discarding) the audio before it. This reader
/// takes exactly one block and hangs up; the caller re-reads periodically
/// instead of keeping a second full copy of the stream open.
///
/// Only one read is active at a time. Each read carries an identity so the
/// cancellation callback of an abandoned read (which arrives *after* the next
/// read has started) cannot finish or tear down the newer one.
final class ICYTitleReader: NSObject, URLSessionDataDelegate {
    private var completion: ((TrackUpdate) -> Void)?
    private var session: URLSession?
    private var currentTask: URLSessionDataTask?
    private var activeRead: UUID?
    private var timeoutWorkItem: DispatchWorkItem?

    private var metaint = 0
    private var remainingAudioBytes = 0
    private var buffer: [UInt8] = []
    private var finished = true

    func readTitle(from url: URL, timeout: TimeInterval = 12, completion: @escaping (TrackUpdate) -> Void) {
        stop()

        let read = UUID()
        activeRead = read
        finished = false
        metaint = 0
        remainingAudioBytes = 0
        buffer.removeAll(keepingCapacity: false)
        self.completion = completion

        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout

        let session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
        self.session = session

        var request = URLRequest(url: url)
        request.setValue("1", forHTTPHeaderField: "Icy-MetaData")
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        let task = session.dataTask(with: request)
        currentTask = task
        task.resume()

        // Never wait on a stream that accepts connections but sends nothing.
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.activeRead == read else { return }
            self.finish(with: .unavailable)
        }
        timeoutWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + timeout, execute: work)
    }

    /// Abandons the current read without reporting anything.
    func stop() {
        finish(with: .unavailable)
    }

    // MARK: - URLSession

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive response: URLResponse,
        completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
    ) {
        guard dataTask === currentTask else {
            completionHandler(.cancel)
            return
        }

        guard
            let http = response as? HTTPURLResponse,
            let header = http.value(forHTTPHeaderField: "icy-metaint"),
            let interval = Int(header),
            interval > 0,
            (200..<300).contains(http.statusCode)
        else {
            // Not an ICY stream (HLS, plain file, error): nothing to read.
            completionHandler(.cancel)
            finish(with: .unavailable)
            return
        }

        metaint = interval
        remainingAudioBytes = interval
        completionHandler(.allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard dataTask === currentTask, metaint > 0, !finished else { return }
        buffer.append(contentsOf: data)

        while !finished {
            // Skip the audio that precedes the next metadata block.
            if remainingAudioBytes > 0 {
                let count = min(remainingAudioBytes, buffer.count)
                guard count > 0 else { return }
                buffer.removeFirst(count)
                remainingAudioBytes -= count
                continue
            }

            guard let lengthByte = buffer.first else { return }
            let blockLength = Int(lengthByte) * 16

            if blockLength == 0 {
                // No metadata in this interval; read on for the next block.
                buffer.removeFirst(1)
                remainingAudioBytes = metaint
                continue
            }

            guard buffer.count >= blockLength + 1 else { return }

            let block = Array(buffer[1...blockLength])
            buffer.removeFirst(blockLength + 1)
            remainingAudioBytes = metaint

            // An empty block is an answer: the station has no song right now.
            if let title = Self.streamTitle(in: block) {
                finish(with: .title(title))
            } else {
                finish(with: .noTitle)
            }
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard task === currentTask else { return }
        finish(with: .unavailable)
    }

    // MARK: - Parsing

    private static func streamTitle(in block: [UInt8]) -> String? {
        let text = String(decoding: block, as: UTF8.self)
        guard
            let start = text.range(of: "StreamTitle='"),
            let end = text[start.upperBound...].firstIndex(of: "'")
        else { return nil }

        let title = text[start.upperBound..<end].trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? nil : title
    }

    private func finish(with result: TrackUpdate) {
        guard !finished else { return }
        finished = true
        activeRead = nil

        timeoutWorkItem?.cancel()
        timeoutWorkItem = nil

        currentTask?.cancel()
        currentTask = nil
        session?.invalidateAndCancel()
        session = nil
        buffer.removeAll(keepingCapacity: false)

        let completion = self.completion
        self.completion = nil
        completion?(result)
    }
}
