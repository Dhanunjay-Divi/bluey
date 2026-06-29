import AppKit
import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

private enum CaptureSource: String {
    case system
    case microphone
}

private enum Mode {
    case capture
    /// Interactive: present SCContentSharingPicker, print the chosen app's
    /// bundle id to stdout, exit. Requires a GUI run loop (macOS 14+).
    case pick
}

private struct Args {
    var source: CaptureSource = .system
    var durationMs: Int = 3_000
    var continuous: Bool = false
    var mode: Mode = .capture
    /// When set (system capture), restrict capture to this app's audio only,
    /// instead of the whole display. Chosen via `--pick`.
    var appBundleId: String?
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
        case "--pick":
            parsed.mode = .pick
        case "--app-bundle":
            if let value = iterator.next(), !value.isEmpty {
                parsed.appBundleId = value
            }
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
    private var carry: Double = 0.0

    func writeMonoFloat(_ samples: [Float], sourceSampleRate: Double) {
        guard !samples.isEmpty, sourceSampleRate > 0 else { return }
        lock.lock()
        var output = Data()
        output.reserveCapacity(samples.count * 2)
        let ratio = 16_000.0 / sourceSampleRate
        for sample in samples {
            carry += ratio
            while carry >= 1.0 {
                let clamped = max(-1.0, min(1.0, sample.isFinite ? sample : 0.0))
                var sample = Int16(clamped * 32767.0)
                withUnsafeBytes(of: &sample) { output.append(contentsOf: $0) }
                carry -= 1.0
            }
        }
        handle.write(output)
        lock.unlock()
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
    private let appBundleId: String?
    private let writer = PCM16Writer()
    private var stream: SCStream?

    init(durationMs: Int, continuous: Bool, appBundleId: String?) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
        self.appBundleId = appBundleId
        super.init()
    }

    func run() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let display = content.displays.first else {
            throw NSError(domain: "BlueyAudio", code: 1, userInfo: [NSLocalizedDescriptionKey: "no display available for ScreenCaptureKit audio"])
        }

        // Per-app filter when a bundle id was chosen (via --pick): capture ONLY
        // that app's audio (the call), not the whole display. Falls back to the
        // whole display when no app is specified or the app isn't running.
        let filter: SCContentFilter
        if let bundleId = appBundleId,
            let app = content.applications.first(where: { $0.bundleIdentifier == bundleId })
        {
            filter = SCContentFilter(
                display: display,
                including: [app],
                exceptingWindows: []
            )
        } else {
            if appBundleId != nil {
                fputs("requested app not running; capturing whole-display audio\n", stderr)
            }
            filter = SCContentFilter(display: display, excludingWindows: [])
        }
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

/// Interactive source picker + capture: presents the macOS system
/// content-sharing picker so the user chooses what to capture, then captures the
/// audio of the CHOSEN filter directly (same process — no need to serialize the
/// filter across processes or read macOS-15-only filter properties). Streams
/// PCM16 to stdout exactly like the non-interactive system path. Requires
/// macOS 14 (SCContentSharingPicker) and a GUI run loop.
@available(macOS 14.0, *)
private final class SourcePicker: NSObject, SCContentSharingPickerObserver, SCStreamOutput {
    private let picker = SCContentSharingPicker.shared
    private let writer = PCM16Writer()
    private var stream: SCStream?

    func present() {
        var config = SCContentSharingPickerConfiguration()
        // Audio filters at the application level, so steer to a single app.
        config.allowedPickerModes = [.singleApplication]
        picker.configuration = config
        picker.add(self)
        picker.isActive = true
        picker.present()
    }

    func contentSharingPicker(
        _ picker: SCContentSharingPicker,
        didUpdateWith filter: SCContentFilter,
        for _: SCStream?
    ) {
        // Capture the chosen filter's audio directly. Hide the picker now that a
        // choice is made; keep streaming until the process is killed.
        picker.isActive = false
        startCapture(with: filter)
    }

    func contentSharingPicker(
        _ picker: SCContentSharingPicker,
        didCancelFor _: SCStream?
    ) {
        fputs("picker cancelled\n", stderr)
        picker.isActive = false
        picker.remove(self)
        exit(4)
    }

    func contentSharingPickerStartDidFailWithError(_ error: Error) {
        fputs("picker failed: \(error.localizedDescription)\n", stderr)
        exit(1)
    }

    private func startCapture(with filter: SCContentFilter) {
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
        do {
            try stream.addStreamOutput(
                self,
                type: .audio,
                sampleHandlerQueue: DispatchQueue(label: "sh.bluey.audio.picked", qos: .userInitiated)
            )
            stream.startCapture { error in
                if let error = error {
                    fputs("picked-capture start failed: \(error.localizedDescription)\n", stderr)
                    exit(1)
                }
            }
        } catch {
            fputs("picked-capture setup failed: \(error.localizedDescription)\n", stderr)
            exit(1)
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

private func run() async -> Int32 {
    let args = parseArgs()

    if args.mode == .pick {
        guard #available(macOS 14.0, *) else {
            fputs("app picker requires macOS 14+\n", stderr)
            return 2
        }
        // Become a foreground app so the system picker can present + focus.
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let picker = SourcePicker()
        picker.present()
        // The picker's delegate callbacks call exit(); run the loop meanwhile.
        app.run()
        return 0
    }

    do {
        switch args.source {
        case .system:
            guard #available(macOS 13.0, *) else {
                fputs("system audio capture requires macOS 13+\n", stderr)
                return 2
            }
            let capture = SystemAudioCapture(
                durationMs: args.durationMs,
                continuous: args.continuous,
                appBundleId: args.appBundleId
            )
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
