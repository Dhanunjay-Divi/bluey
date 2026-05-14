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

/// Writes 16 kHz mono i16 LE PCM to stdout from 48 kHz mono float input.
private final class PCM16Writer {
    private let handle = FileHandle.standardOutput
    private let lock = NSLock()
    private let ratio: Double = 16_000.0 / 48_000.0
    private var carry: Double = 0.0

    func write48kFloat(_ pointer: UnsafePointer<Float>, frameCount: Int) {
        guard frameCount > 0 else { return }
        lock.lock()
        var output = Data()
        output.reserveCapacity(frameCount * 2 / 3)
        for i in 0..<frameCount {
            carry += ratio
            while carry >= 1.0 {
                let clamped = max(-1.0, min(1.0, pointer[i]))
                var sample = Int16(clamped * 32767.0)
                withUnsafeBytes(of: &sample) { output.append(contentsOf: $0) }
                carry -= 1.0
            }
        }
        handle.write(output)
        lock.unlock()
    }

    func writePlanar48kFloat(_ bufferList: UnsafeMutableAudioBufferListPointer, frameCount: Int) {
        guard frameCount > 0 else { return }
        if bufferList.count == 1,
           let data = bufferList[0].mData?.assumingMemoryBound(to: Float.self) {
            write48kFloat(data, frameCount: frameCount)
            return
        }
        var mono = [Float](repeating: 0, count: frameCount)
        for audioBuffer in bufferList {
            guard let data = audioBuffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
            for index in 0..<frameCount {
                mono[index] += data[index] / Float(max(bufferList.count, 1))
            }
        }
        mono.withUnsafeBufferPointer { pointer in
            if let base = pointer.baseAddress {
                write48kFloat(base, frameCount: frameCount)
            }
        }
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
        writer.writePlanar48kFloat(
            UnsafeMutableAudioBufferListPointer(&audioBufferList),
            frameCount: sampleBuffer.numSamples
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

    func run() throws {
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.channelCount > 0 else {
            throw NSError(domain: "BlueyAudio", code: 2, userInfo: [NSLocalizedDescriptionKey: "no microphone input format available"])
        }
        guard let targetFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48_000, channels: 1, interleaved: false) else {
            throw NSError(domain: "BlueyAudio", code: 3, userInfo: [NSLocalizedDescriptionKey: "failed to create target format"])
        }
        let converter = AVAudioConverter(from: inputFormat, to: targetFormat)

        input.installTap(onBus: 0, bufferSize: 1_024, format: inputFormat) { [weak self] buffer, _ in
            guard let self else { return }
            let ratio = targetFormat.sampleRate / buffer.format.sampleRate
            let capacity = AVAudioFrameCount(max(1, Int(Double(buffer.frameLength) * ratio) + 8))
            guard let converted = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return }
            var consumed = false
            converter?.convert(to: converted, error: nil) { _, status in
                if consumed { status.pointee = .noDataNow; return nil }
                consumed = true
                status.pointee = .haveData
                return buffer
            }
            guard converted.frameLength > 0, let channel = converted.floatChannelData?[0] else { return }
            self.writer.write48kFloat(channel, frameCount: Int(converted.frameLength))
        }
        try engine.start()

        if continuous {
            // Run until killed
            dispatchMain()
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
