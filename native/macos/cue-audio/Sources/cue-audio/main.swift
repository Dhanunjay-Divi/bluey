import AppKit
import AudioToolbox
import AVFoundation
import CoreAudio
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

/// System-audio capture via the Core Audio process-tap API (macOS 14.2+).
///
/// The ScreenCaptureKit audio path (`SCStream` with `capturesAudio`) delivers
/// intermittent SILENT buffers on macOS 26.5 (Tahoe) on this machine, so we use
/// the documented-reliable Core Audio tap path instead:
///   1. `AudioHardwareCreateProcessTap` — a global stereo tap.
///   2. An aggregate device whose tap list includes that tap, with the system
///      default output as its main sub-device.
///   3. An IOProc on the aggregate device reads the tap's Float32 buffers; we
///      down-mix to mono and feed `PCM16Writer.writeMonoFloat` at the tap's
///      real sample rate (read from `kAudioTapPropertyFormat`, NOT hardcoded).
///
/// Output on stdout is identical to the old path: 16 kHz mono i16 LE PCM.
@available(macOS 13.0, *)
private final class SystemAudioCapture {
    private let duration: TimeInterval
    private let continuous: Bool
    private let appBundleId: String?
    private let writer = PCM16Writer()

    // Core Audio resources owned by this capture; torn down on cleanup.
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    /// The tap's real stream sample rate (often 48 kHz), read from the tap.
    private var tapSampleRate: Double = 48_000

    init(durationMs: Int, continuous: Bool, appBundleId: String?) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
        self.appBundleId = appBundleId
    }

    func run() async throws {
        guard #available(macOS 14.2, *) else {
            throw NSError(
                domain: "BlueyAudio",
                code: 10,
                userInfo: [NSLocalizedDescriptionKey: "Core Audio process-tap capture requires macOS 14.2+"]
            )
        }

        // The Core Audio tap captures all system output regardless of which app
        // produced it; per-app scoping is only available via the --pick path.
        if appBundleId != nil {
            fputs("note: Core Audio tap captures whole-system audio; per-app scope ignored\n", stderr)
        }

        try setUpTap()

        if continuous {
            // Run until killed.
            while !Task.isCancelled {
                try await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } else {
            try await Task.sleep(nanoseconds: UInt64(duration * 1_000_000_000))
            tearDown()
        }
    }

    // MARK: - Tap setup

    /// Our own process as a Core Audio object id, for the tap's exclude list.
    ///
    /// `CATapDescription(stereoGlobalTapButExcludeProcesses:)` takes audio
    /// OBJECT ids, not pids — passing a pid would silently exclude an unrelated
    /// process (or nothing). Core Audio exposes the mapping via
    /// `kAudioHardwarePropertyTranslatePIDToProcessObject`.
    ///
    /// Returns nil when the translation fails, which is normal: a process that
    /// has never produced audio has no process object yet. The caller then taps
    /// globally, which is the pre-existing behavior.
    private static func ownAudioProcessObjectID() -> AudioObjectID? {
        var pid = ProcessInfo.processInfo.processIdentifier
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var objectID = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            UInt32(MemoryLayout<pid_t>.size),
            &pid,
            &size,
            &objectID
        )
        guard status == noErr, objectID != AudioObjectID(kAudioObjectUnknown) else {
            return nil
        }
        return objectID
    }

    @available(macOS 14.2, *)
    private func setUpTap() throws {
        // 1. Tap description: a private, unmuted, global stereo tap that
        //    EXCLUDES our own audio.
        //
        // The exclude list was previously empty, so the tap captured output
        // from every process INCLUDING this one. Anything Bluey itself plays is
        // then re-captured and re-transcribed, which is the same echo-loop the
        // ScreenCaptureKit path avoids with `excludesCurrentProcessAudio` (see
        // the picker path below). Excluding our own PID is the Core Audio
        // equivalent.
        //
        // Note this is the OUTPUT side only: it stops Bluey's own playback from
        // looping back. Speaker-to-microphone bleed is a separate acoustic path
        // and is handled by the VoiceProcessingIO AEC on the mic input.
        // The API takes audio-object IDs, NOT pids, so translate ours first
        // (`kAudioHardwarePropertyTranslatePIDToProcessObject`). If the
        // translation fails — which it does when this process has never played
        // audio and so has no process object — fall back to an empty exclude
        // list: a global tap is still far better than no capture at all.
        let excluded = Self.ownAudioProcessObjectID().map { [$0] } ?? []
        let tapDescription = CATapDescription(stereoGlobalTapButExcludeProcesses: excluded)
        tapDescription.uuid = UUID()
        tapDescription.muteBehavior = .unmuted
        tapDescription.isPrivate = true
        tapDescription.name = "BlueyAudioTap"

        // 2. Create the process tap.
        var newTapID = AudioObjectID(kAudioObjectUnknown)
        let tapErr = AudioHardwareCreateProcessTap(tapDescription, &newTapID)
        guard tapErr == noErr, newTapID != AudioObjectID(kAudioObjectUnknown) else {
            throw NSError(
                domain: "BlueyAudio",
                code: 11,
                userInfo: [NSLocalizedDescriptionKey: "AudioHardwareCreateProcessTap failed (OSStatus \(tapErr))"]
            )
        }
        tapID = newTapID

        // 3. Default output device UID — the aggregate needs a real output as its
        //    main sub-device.
        let outputUID: String
        do {
            outputUID = try defaultOutputDeviceUID()
        } catch {
            tearDown()
            throw error
        }

        // 5. (done before building the IOProc) Tap stream format → real sample
        //    rate + channel count.
        if let format = try? tapStreamFormat(), format.mSampleRate > 0 {
            tapSampleRate = format.mSampleRate
        } else {
            fputs("warning: could not read tap format; defaulting to 48 kHz\n", stderr)
        }

        // 4. Aggregate device whose tap list includes the tap.
        let aggUID = UUID().uuidString
        let desc: [String: Any] = [
            kAudioAggregateDeviceNameKey: "BlueyAudioAggregate",
            kAudioAggregateDeviceUIDKey: aggUID,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [
                [kAudioSubDeviceUIDKey: outputUID]
            ],
            kAudioAggregateDeviceTapListKey: [
                [
                    kAudioSubTapUIDKey: tapDescription.uuid.uuidString,
                    kAudioSubTapDriftCompensationKey: true,
                ]
            ],
        ]
        var newAggregateID = AudioObjectID(kAudioObjectUnknown)
        let aggErr = AudioHardwareCreateAggregateDevice(desc as CFDictionary, &newAggregateID)
        guard aggErr == noErr, newAggregateID != AudioObjectID(kAudioObjectUnknown) else {
            tearDown()
            throw NSError(
                domain: "BlueyAudio",
                code: 12,
                userInfo: [NSLocalizedDescriptionKey: "AudioHardwareCreateAggregateDevice failed (OSStatus \(aggErr))"]
            )
        }
        aggregateID = newAggregateID

        // 6. IOProc on the aggregate device reading the tap buffers.
        let queue = DispatchQueue(label: "sh.bluey.audio.tap", qos: .userInteractive)
        let writer = self.writer
        let sampleRate = self.tapSampleRate
        var newProcID: AudioDeviceIOProcID?
        let ioErr = AudioDeviceCreateIOProcIDWithBlock(&newProcID, aggregateID, queue) {
            _, inInputData, _, _, _ in
            SystemAudioCapture.handleInput(inInputData, writer: writer, sourceSampleRate: sampleRate)
        }
        guard ioErr == noErr, let createdProcID = newProcID else {
            tearDown()
            throw NSError(
                domain: "BlueyAudio",
                code: 13,
                userInfo: [NSLocalizedDescriptionKey: "AudioDeviceCreateIOProcIDWithBlock failed (OSStatus \(ioErr))"]
            )
        }
        procID = createdProcID

        let startErr = AudioDeviceStart(aggregateID, createdProcID)
        guard startErr == noErr else {
            tearDown()
            throw NSError(
                domain: "BlueyAudio",
                code: 14,
                userInfo: [NSLocalizedDescriptionKey: "AudioDeviceStart failed (OSStatus \(startErr))"]
            )
        }
    }

    /// Walks the tap's input `AudioBufferList`, down-mixes Float32 samples to
    /// mono, and feeds the resampling writer. Handles interleaved stereo,
    /// non-interleaved (multi-buffer) stereo, and mono.
    private static func handleInput(
        _ inInputData: UnsafePointer<AudioBufferList>,
        writer: PCM16Writer,
        sourceSampleRate: Double
    ) {
        let bufferList = UnsafeMutableAudioBufferListPointer(
            UnsafeMutablePointer(mutating: inInputData)
        )
        guard bufferList.count > 0 else { return }

        if bufferList.count > 1 {
            // Non-interleaved: one buffer per channel. Average channel 0..N.
            let firstChannels = max(Int(bufferList[0].mNumberChannels), 1)
            let frameCount = Int(bufferList[0].mDataByteSize) / 4 / firstChannels
            guard frameCount > 0 else { return }
            var mono = [Float](repeating: 0, count: frameCount)
            var channelsMixed = 0
            for bufferIndex in 0..<bufferList.count {
                guard let data = bufferList[bufferIndex].mData?.assumingMemoryBound(to: Float.self) else { continue }
                let channels = max(Int(bufferList[bufferIndex].mNumberChannels), 1)
                let frames = Int(bufferList[bufferIndex].mDataByteSize) / 4 / channels
                let usable = min(frames, frameCount)
                // Each non-interleaved buffer typically holds one channel.
                for frame in 0..<usable {
                    mono[frame] += data[frame]
                }
                channelsMixed += 1
            }
            guard channelsMixed > 0 else { return }
            if channelsMixed > 1 {
                let divisor = Float(channelsMixed)
                for index in mono.indices { mono[index] /= divisor }
            }
            writer.writeMonoFloat(mono, sourceSampleRate: sourceSampleRate)
        } else {
            // Single buffer: mono or interleaved stereo/N-channel.
            let buffer = bufferList[0]
            guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { return }
            let channels = max(Int(buffer.mNumberChannels), 1)
            let frameCount = Int(buffer.mDataByteSize) / 4 / channels
            guard frameCount > 0 else { return }
            if channels == 1 {
                let mono = Array(UnsafeBufferPointer(start: data, count: frameCount))
                writer.writeMonoFloat(mono, sourceSampleRate: sourceSampleRate)
            } else {
                var mono = [Float](repeating: 0, count: frameCount)
                let divisor = Float(channels)
                for frame in 0..<frameCount {
                    var sum: Float = 0
                    for channel in 0..<channels {
                        sum += data[frame * channels + channel]
                    }
                    mono[frame] = sum / divisor
                }
                writer.writeMonoFloat(mono, sourceSampleRate: sourceSampleRate)
            }
        }
    }

    // MARK: - Core Audio helpers

    /// Returns the UID of the system default OUTPUT device.
    private func defaultOutputDeviceUID() throws -> String {
        var deviceID = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let devErr = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &deviceID
        )
        guard devErr == noErr, deviceID != AudioObjectID(kAudioObjectUnknown) else {
            throw NSError(
                domain: "BlueyAudio",
                code: 15,
                userInfo: [NSLocalizedDescriptionKey: "no default output device (OSStatus \(devErr))"]
            )
        }

        var uidRef: CFString = "" as CFString
        var uidSize = UInt32(MemoryLayout<CFString?>.size)
        var uidAddr = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceUID,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let uidErr = withUnsafeMutablePointer(to: &uidRef) { ptr -> OSStatus in
            AudioObjectGetPropertyData(deviceID, &uidAddr, 0, nil, &uidSize, ptr)
        }
        guard uidErr == noErr else {
            throw NSError(
                domain: "BlueyAudio",
                code: 16,
                userInfo: [NSLocalizedDescriptionKey: "could not read default output device UID (OSStatus \(uidErr))"]
            )
        }
        return uidRef as String
    }

    /// Reads the tap's stream format (`kAudioTapPropertyFormat`).
    @available(macOS 14.2, *)
    private func tapStreamFormat() throws -> AudioStreamBasicDescription {
        var format = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioTapPropertyFormat,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let err = AudioObjectGetPropertyData(tapID, &addr, 0, nil, &size, &format)
        guard err == noErr else {
            throw NSError(
                domain: "BlueyAudio",
                code: 17,
                userInfo: [NSLocalizedDescriptionKey: "could not read tap format (OSStatus \(err))"]
            )
        }
        return format
    }

    // MARK: - Cleanup

    /// Best-effort teardown of all Core Audio resources, in reverse order.
    private func tearDown() {
        if aggregateID != AudioObjectID(kAudioObjectUnknown), let procID = procID {
            AudioDeviceStop(aggregateID, procID)
            AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        procID = nil
        if aggregateID != AudioObjectID(kAudioObjectUnknown) {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = AudioObjectID(kAudioObjectUnknown)
        }
        if tapID != AudioObjectID(kAudioObjectUnknown) {
            if #available(macOS 14.2, *) {
                AudioHardwareDestroyProcessTap(tapID)
            }
            tapID = AudioObjectID(kAudioObjectUnknown)
        }
    }

    deinit {
        tearDown()
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

    /// Enables Apple's VoiceProcessingIO acoustic echo cancellation on the input
    /// node, so the far side's voice — played through the speakers and leaking
    /// back into the mic — is removed BEFORE it reaches STT. Without this the
    /// same speech is transcribed twice: once from the system-audio tap and
    /// again from the mic. Text-level dedup cannot fix that reliably; hardware
    /// AEC is what production conferencing apps use.
    ///
    /// Returns true if AEC is active. Fail-soft by design: a mic with echo is
    /// far better than no mic, so every failure path logs and returns false.
    private func enableEchoCancellation(on input: AVAudioInputNode) -> Bool {
        // Escape hatch: voice processing also applies AGC + noise suppression,
        // which alters voice timbre. A user on a headset has no echo path to
        // cancel and may prefer the untouched signal.
        if ProcessInfo.processInfo.environment["BLUEY_AEC"] == "0" {
            fputs("microphone: echo cancellation disabled by BLUEY_AEC=0\n", stderr)
            return false
        }
        do {
            // Must run before the engine starts and before the tap is installed:
            // switching to the VoiceProcessingIO unit reconfigures the input
            // hardware, which is only legal on a stopped engine.
            try input.setVoiceProcessingEnabled(true)

            // CRITICAL: turn OFF the ducking VoiceProcessingIO applies by
            // default. The unit assumes it is powering a voice call, so it
            // aggressively attenuates all OTHER audio on the machine while the
            // mic is live. Bluey's whole second capture path IS that other
            // audio (the system tap recording the far side), so leaving the
            // default on trades duplicate transcripts for a near-silent system
            // stream — one independent report measured -51 dB, effectively
            // inaudible. `.min` keeps the smallest attenuation the API allows,
            // and advanced (voice-activity-driven) ducking stays off so levels
            // do not pump while people talk.
            if #available(macOS 14.0, *) {
                input.voiceProcessingOtherAudioDuckingConfiguration =
                    AVAudioVoiceProcessingOtherAudioDuckingConfiguration(
                        enableAdvancedDucking: false,
                        duckingLevel: .min
                    )
            }

            fputs("microphone: echo cancellation ENABLED (ducking minimized)\n", stderr)
            return true
        } catch {
            fputs(
                "microphone: echo cancellation unavailable (\(error.localizedDescription)) — continuing without\n",
                stderr
            )
            return false
        }
    }

    func run() throws {
        let input = engine.inputNode
        // Enable AEC BEFORE reading the input format: the VoiceProcessingIO unit
        // imposes its own sample rate and channel count, so a format captured
        // beforehand would no longer describe the buffers the tap receives and
        // the converter would emit garbled audio.
        _ = enableEchoCancellation(on: input)
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

    // PERMISSION MODEL — this matters, and was subtly WRONG before.
    //
    // The system-audio path uses the Core Audio process-tap
    // (`AudioHardwareCreateProcessTap`, macOS 14.4+). On 14.4+ that tap is gated
    // by the NEWER **"System Audio Recording Only"** TCC permission
    // (`NSAudioCaptureUsageDescription`) — NOT "Screen & System Audio Recording"
    // (`CGRequestScreenCaptureAccess` / kTCCServiceScreenCapture). The tap
    // requests the correct permission on its own first use.
    //
    // The OLD code gated EVERY mode on `CGRequestScreenCaptureAccess` — a
    // leftover from the retired ScreenCaptureKit path. That checked the WRONG
    // list: the user could grant Screen Recording and still be denied (the tap
    // needs the Audio-Recording grant), and the dialog kept firing for a
    // permission the tap never uses. So we DO NOT gate the tap on Screen
    // Recording here. Only the interactive `--pick` mode uses ScreenCaptureKit
    // (`SCContentSharingPicker`), which genuinely needs Screen Recording, so the
    // gate is scoped to that mode below.
    if args.mode == .pick {
        if #available(macOS 11.0, *) {
            if !CGPreflightScreenCaptureAccess() {
                fputs("screen recording permission not granted — requesting…\n", stderr)
                if !CGRequestScreenCaptureAccess() {
                    fputs(
                        "ERROR: screen recording permission DENIED (needed for the app picker). Grant it in System Settings → Privacy & Security → Screen Recording, then relaunch.\n",
                        stderr
                    )
                    return 3
                }
            }
        }
    }

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
