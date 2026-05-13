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
        default:
            break
        }
    }
    return parsed
}

private final class RawFloatWriter {
    private let handle = FileHandle.standardOutput
    private let lock = NSLock()

    func writeMonoFloat32(_ pointer: UnsafePointer<Float>, frameCount: Int) {
        guard frameCount > 0 else { return }
        lock.lock()
        handle.write(Data(bytes: pointer, count: frameCount * MemoryLayout<Float>.size))
        lock.unlock()
    }

    func writeInterleavedOrPlanarFloat32(_ bufferList: UnsafeMutableAudioBufferListPointer, frameCount: Int) {
        guard frameCount > 0 else { return }
        if bufferList.count == 1,
           let data = bufferList[0].mData?.assumingMemoryBound(to: Float.self) {
            writeMonoFloat32(data, frameCount: frameCount)
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
                writeMonoFloat32(base, frameCount: frameCount)
            }
        }
    }
}

@available(macOS 13.0, *)
private final class SystemAudioCapture: NSObject, SCStreamOutput {
    private let duration: TimeInterval
    private let writer = RawFloatWriter()
    private var stream: SCStream?

    init(durationMs: Int) {
        self.duration = TimeInterval(durationMs) / 1_000.0
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
        try await Task.sleep(nanoseconds: UInt64(duration * 1_000_000_000))
        try await stream.stopCapture()
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
        writer.writeInterleavedOrPlanarFloat32(
            UnsafeMutableAudioBufferListPointer(&audioBufferList),
            frameCount: sampleBuffer.numSamples
        )
        _ = blockBuffer
    }
}

private final class MicrophoneCapture {
    private let duration: TimeInterval
    private let writer = RawFloatWriter()
    private let engine = AVAudioEngine()
    private var converter: AVAudioConverter?
    private var targetFormat: AVAudioFormat?

    init(durationMs: Int) {
        self.duration = TimeInterval(durationMs) / 1_000.0
    }

    func run() throws {
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.channelCount > 0 else {
            throw NSError(domain: "BlueyAudio", code: 2, userInfo: [NSLocalizedDescriptionKey: "no microphone input format available"])
        }
        guard let targetFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48_000, channels: 1, interleaved: false) else {
            throw NSError(domain: "BlueyAudio", code: 3, userInfo: [NSLocalizedDescriptionKey: "failed to create microphone target format"])
        }
        self.targetFormat = targetFormat
        converter = AVAudioConverter(from: inputFormat, to: targetFormat)

        input.installTap(onBus: 0, bufferSize: 1_024, format: inputFormat) { [weak self] buffer, _ in
            self?.handle(buffer: buffer)
        }
        try engine.start()
        Thread.sleep(forTimeInterval: duration)
        engine.stop()
        input.removeTap(onBus: 0)
    }

    private func handle(buffer: AVAudioPCMBuffer) {
        guard let targetFormat else { return }
        guard let converter else { return }
        let ratio = targetFormat.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(max(1, Int(Double(buffer.frameLength) * ratio) + 8))
        guard let converted = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return }

        var consumed = false
        var conversionError: NSError?
        converter.convert(to: converted, error: &conversionError) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }

        guard conversionError == nil,
              converted.frameLength > 0,
              let channel = converted.floatChannelData?[0] else { return }
        writer.writeMonoFloat32(channel, frameCount: Int(converted.frameLength))
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
            let capture = SystemAudioCapture(durationMs: args.durationMs)
            try await capture.run()
        case .microphone:
            let capture = MicrophoneCapture(durationMs: args.durationMs)
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
