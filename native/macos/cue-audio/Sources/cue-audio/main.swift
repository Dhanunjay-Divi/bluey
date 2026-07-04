import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

private enum CaptureSource: String {
    case system
    case microphone
}

private struct Args {
    var source: CaptureSource = .system
    var durationMs: Int = 3_000
    var continuous: Bool = false
}

private func parseArgs() -> Args {
    var parsed = Args()
    var iterator = CommandLine.arguments.dropFirst().makeIterator()
    while let arg = iterator.next() {
        switch arg {
        case "--source":
            if let value = iterator.next(), let source = CaptureSource(rawValue: value) {
                parsed.source = source
            }
        case "--duration-ms":
            if let value = iterator.next(), let duration = Int(value) {
                parsed.durationMs = min(max(duration, 250), 30_000)
            }
        case "--continuous":
            parsed.continuous = true
        default:
            break
        }
    }
    return parsed
}

/// Writes 16 kHz mono i16 LE PCM to stdout from mono float input.
private final class PCM16Writer {
    private let handle = FileHandle.standardOutput
    private let lock = NSLock()
    private var carry: Double = 0.0
    private var windowSum: Double = 0.0
    private var windowCount: Int = 0
    private var lastSourceSampleRate: Double = 0.0

    func writeMonoFloat(_ samples: [Float], sourceSampleRate: Double) {
        guard !samples.isEmpty, sourceSampleRate > 0 else { return }
        lock.lock()
        defer { lock.unlock() }
        if abs(sourceSampleRate - lastSourceSampleRate) > 0.1 {
            carry = 0.0
            windowSum = 0.0
            windowCount = 0
            lastSourceSampleRate = sourceSampleRate
        }
        var output = Data()
        output.reserveCapacity(Int((Double(samples.count) * 16_000.0 / sourceSampleRate + 2.0) * 2.0))
        let samplesPerOutput = max(sourceSampleRate / 16_000.0, 0.001)
        for sample in samples {
            let clamped = max(-1.0, min(1.0, sample.isFinite ? sample : 0.0))
            windowSum += Double(clamped)
            windowCount += 1
            carry += 1.0
            while carry >= samplesPerOutput {
                let averaged = windowCount > 0 ? windowSum / Double(windowCount) : Double(clamped)
                let bounded = max(-1.0, min(1.0, averaged))
                var sample = Int16(bounded * 32767.0)
                withUnsafeBytes(of: &sample) { output.append(contentsOf: $0) }
                carry -= samplesPerOutput
                windowSum = 0.0
                windowCount = 0
            }
        }
        handle.write(output)
    }

    func write48kFloat(_ pointer: UnsafePointer<Float>, frameCount: Int) {
        guard frameCount > 0 else { return }
        writeMonoFloat(Array(UnsafeBufferPointer(start: pointer, count: frameCount)), sourceSampleRate: 48_000)
    }

    func writePCM(
        _ bufferList: UnsafeMutableAudioBufferListPointer,
        frameCount: Int,
        format: AudioStreamBasicDescription
    ) {
        guard frameCount > 0 else { return }
        let channelCount = max(Int(format.mChannelsPerFrame), 1)
        let sourceSampleRate = format.mSampleRate > 0 ? format.mSampleRate : 48_000
        let flags = format.mFormatFlags
        let isFloat = (flags & kAudioFormatFlagIsFloat) != 0
        let isSignedInt = (flags & kAudioFormatFlagIsSignedInteger) != 0
        let isNonInterleaved = (flags & kAudioFormatFlagIsNonInterleaved) != 0

        var mono = [Float](repeating: 0, count: frameCount)
        var channelsMixed = 0

        if isFloat && format.mBitsPerChannel == 32 {
            if isNonInterleaved || bufferList.count > 1 {
                let buffersToRead = min(bufferList.count, channelCount)
                for bufferIndex in 0..<buffersToRead {
                    guard let data = bufferList[bufferIndex].mData?.assumingMemoryBound(to: Float.self) else { continue }
                    for frame in 0..<frameCount {
                        mono[frame] += data[frame]
                    }
                    channelsMixed += 1
                }
            } else if let data = bufferList.first?.mData?.assumingMemoryBound(to: Float.self) {
                for frame in 0..<frameCount {
                    var sum: Float = 0
                    for channel in 0..<channelCount {
                        sum += data[frame * channelCount + channel]
                    }
                    mono[frame] = sum
                }
                channelsMixed = channelCount
            }
        } else if isSignedInt && format.mBitsPerChannel == 16 {
            if isNonInterleaved || bufferList.count > 1 {
                let buffersToRead = min(bufferList.count, channelCount)
                for bufferIndex in 0..<buffersToRead {
                    guard let data = bufferList[bufferIndex].mData?.assumingMemoryBound(to: Int16.self) else { continue }
                    for frame in 0..<frameCount {
                        mono[frame] += Float(data[frame]) / 32768.0
                    }
                    channelsMixed += 1
                }
            } else if let data = bufferList.first?.mData?.assumingMemoryBound(to: Int16.self) {
                for frame in 0..<frameCount {
                    var sum: Float = 0
                    for channel in 0..<channelCount {
                        sum += Float(data[frame * channelCount + channel]) / 32768.0
                    }
                    mono[frame] = sum
                }
                channelsMixed = channelCount
            }
        }

        guard channelsMixed > 0 else { return }
        if channelsMixed > 1 {
            let divisor = Float(channelsMixed)
            for index in mono.indices {
                mono[index] /= divisor
            }
        }
        writeMonoFloat(mono, sourceSampleRate: sourceSampleRate)
    }
}

@available(macOS 13.0, *)
private final class SystemAudioCapture: NSObject, SCStreamOutput {
    private let duration: TimeInterval
    private let continuous: Bool
    private let writer = PCM16Writer()
    private var stream: SCStream?

    init(durationMs: Int, continuous: Bool) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
        super.init()
    }

    func run() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let display = content.displays.first else {
            throw NSError(domain: "BlueyAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: "no display available for ScreenCaptureKit audio"])
        }

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true
        config.sampleRate = 48_000
        config.channelCount = 1
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        config.queueDepth = 3

        let stream = SCStream(filter: filter, configuration: config, delegate: nil)
        self.stream = stream
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: DispatchQueue(label: "sh.bluey.audio.system", qos: .userInitiated))
        try await stream.startCapture()

        if continuous {
            // Run until killed
            while true {
                try await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } else {
            try await Task.sleep(nanoseconds: UInt64(duration * 1_000_000_000))
            try await stream.stopCapture()
        }
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, sampleBuffer.isValid, sampleBuffer.numSamples > 0 else { return }
        var audioBufferList = AudioBufferList()
        var blockBuffer: CMBlockBuffer?
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: &audioBufferList,
            bufferListSize: MemoryLayout<AudioBufferList>.size,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: &blockBuffer
        )
        guard status == noErr else { return }
        let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer)
        guard
            let streamDescription = formatDescription.flatMap({
                CMAudioFormatDescriptionGetStreamBasicDescription($0)
            })?.pointee
        else { return }
        writer.writePCM(
            UnsafeMutableAudioBufferListPointer(&audioBufferList),
            frameCount: sampleBuffer.numSamples,
            format: streamDescription
        )
        _ = blockBuffer
    }
}

private final class MicrophoneCapture {
    private let duration: TimeInterval
    private let continuous: Bool
    private let writer = PCM16Writer()
    private let engine = AVAudioEngine()

    init(durationMs: Int, continuous: Bool) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
    }

    private func ensureMicrophoneAccess() throws {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return
        case .notDetermined:
            let semaphore = DispatchSemaphore(value: 0)
            var granted = false
            AVCaptureDevice.requestAccess(for: .audio) { allowed in
                granted = allowed
                semaphore.signal()
            }
            semaphore.wait()
            if granted {
                return
            }
            fallthrough
        case .denied, .restricted:
            throw NSError(domain: "BlueyAudio", code: 4, userInfo: [NSLocalizedDescriptionKey: "microphone permission is not granted for Bluey audio capture"])
        @unknown default:
            throw NSError(domain: "BlueyAudio", code: 5, userInfo: [NSLocalizedDescriptionKey: "microphone permission status is unknown"])
        }
    }

    func run() throws {
        try ensureMicrophoneAccess()
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.channelCount > 0 else {
            throw NSError(domain: "BlueyAudio", code: 2, userInfo: [NSLocalizedDescriptionKey: "no microphone input format available"])
        }

        input.installTap(onBus: 0, bufferSize: 1_024, format: inputFormat) { [weak self] buffer, _ in
            guard let self else { return }
            let streamDescription = buffer.format.streamDescription.pointee
            self.writer.writePCM(
                UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList),
                frameCount: Int(buffer.frameLength),
                format: streamDescription
            )
        }
        try engine.start()

        if continuous {
            // Keep the AVAudioEngine instance and its tap alive until the
            // daemon terminates the helper. Calling dispatchMain() from this
            // worker task can return immediately on some macOS launches,
            // leaving live microphone capture with zero bytes.
            while true {
                Thread.sleep(forTimeInterval: 1.0)
            }
        } else {
            Thread.sleep(forTimeInterval: duration)
            engine.stop()
            input.removeTap(onBus: 0)
        }
    }
}

private func run() async -> Int32 {
    let args = parseArgs()
    do {
        switch args.source {
        case .system:
            guard #available(macOS 13.0, *) else {
                fputs("system audio capture requires macOS 13+\n", stderr)
                return 2
            }
            let capture = SystemAudioCapture(durationMs: args.durationMs, continuous: args.continuous)
            try await capture.run()
        case .microphone:
            let capture = MicrophoneCapture(durationMs: args.durationMs, continuous: args.continuous)
            try capture.run()
        }
        return 0
    } catch {
        fputs("bluey audio helper failed: \(error.localizedDescription)\n", stderr)
        return 1
    }
}

Task {
    exit(await run())
}
dispatchMain()
