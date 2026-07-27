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

private let permissionDeniedExitCode: Int32 = 3
// Keep this outside the small setup-error range used by capture construction.
private let systemPermissionDeniedErrorCode = 1_001

private func exitCode(for error: Error, source: CaptureSource) -> Int32 {
    let nsError = error as NSError
    guard nsError.domain == "BlueyAudio" else { return 1 }

    switch source {
    case .microphone:
        // Codes 10 and 11 are the explicit denied/restricted authorization
        // failures from ensureMicrophonePermission().
        return [10, 11].contains(nsError.code) ? permissionDeniedExitCode : 1
    case .system:
        // Only the explicit kAudioDevicePermissionsError mapping is a proven
        // privacy denial. Other process-tap failures are setup/audio errors and
        // must not send the user through the grant flow again.
        return nsError.code == systemPermissionDeniedErrorCode
            ? permissionDeniedExitCode : 1
    }
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
    /// Code-signing/LaunchServices smoke path. Exits before touching any capture
    /// API, so install verification never creates a TCC permission prompt.
    var launchProbe: Bool = false
    /// Unique verifier-owned file written by `--launch-probe`. Requiring this
    /// authenticated handshake prevents an older helper that ignores the flag
    /// from being mistaken for a successful no-capture probe.
    var launchProbeOutput: String?
    /// When set (system capture), restrict capture to this app's audio only,
    /// instead of the whole display. Chosen via `--pick`.
    var appBundleId: String?
    /// When set, stream PCM to this UNIX-domain socket instead of stdout. Used by
    /// the MICROPHONE path: the daemon must launch that helper AS the
    /// `BlueyAudio.app` bundle (via `open`) so macOS reads
    /// `NSMicrophoneUsageDescription` from the bundle Info.plist — a bare exec
    /// lacks the plist and macOS TRAPS the instant it touches the mic (exit 133).
    /// But `open` detaches the process and gives no stdout pipe, so the helper
    /// connects back over this socket to deliver PCM. (System audio does not need
    /// this — the Core Audio tap works from a bare exec over stdout.)
    var socketPath: String?
    /// Optional daemon-owned status sidecar for the bundle-launched microphone
    /// path. It distinguishes a proven TCC denial from signing/launch/setup
    /// failures that must not be mislabeled as another permission prompt.
    var statusPath: String?
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
        case "--launch-probe":
            parsed.launchProbe = true
        case "--launch-probe-output":
            if let value = iterator.next(), !value.isEmpty {
                parsed.launchProbeOutput = value
            }
        case "--app-bundle":
            if let value = iterator.next(), !value.isEmpty {
                parsed.appBundleId = value
            }
        case "--socket":
            if let value = iterator.next(), !value.isEmpty {
                parsed.socketPath = value
            }
        case "--status-file":
            if let value = iterator.next(), !value.isEmpty {
                parsed.statusPath = value
            }
        default:
            break
        }
    }
    return parsed
}

private func writeHelperStatus(_ status: String, to outputPath: String?) {
    guard let outputPath, !outputPath.isEmpty else { return }
    let report = """
    \(status)
    pid=\(ProcessInfo.processInfo.processIdentifier)

    """
    do {
        try report.write(
            to: URL(fileURLWithPath: outputPath),
            atomically: true,
            encoding: .utf8
        )
    } catch {
        fputs("bluey audio helper: status handshake failed: \(error)\n", stderr)
    }
}

private func completeLaunchProbe(outputPath: String?) -> Int32 {
    guard let outputPath, !outputPath.isEmpty else {
        fputs("bluey audio helper: --launch-probe-output is required\n", stderr)
        return 5
    }
    let marker = """
    bluey-audio-launch-probe-v1
    pid=\(ProcessInfo.processInfo.processIdentifier)

    """
    do {
        try marker.write(
            to: URL(fileURLWithPath: outputPath),
            atomically: true,
            encoding: .utf8
        )
        return 0
    } catch {
        fputs("bluey audio helper: launch probe handshake failed: \(error)\n", stderr)
        return 5
    }
}

/// Connect to the daemon's UNIX-domain socket and return a `FileHandle` for
/// writing PCM. Returns nil on failure (bad path / daemon not listening); the
/// caller then treats it as fatal (the daemon reads the socket, not stdout, so a
/// stdout fallback would silently drop all audio). The daemon binds+listens
/// BEFORE launching this helper, so a healthy launch connects immediately.
private func connectSocket(_ path: String) -> FileHandle? {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    if fd < 0 { return nil }
    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)
    let pathBytes = Array(path.utf8)
    let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
    if pathBytes.count > maxLen {
        close(fd)
        return nil
    }
    withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
        ptr.withMemoryRebound(to: CChar.self, capacity: maxLen + 1) { dst in
            for (i, b) in pathBytes.enumerated() { dst[i] = CChar(bitPattern: b) }
            dst[pathBytes.count] = 0
        }
    }
    let connected = withUnsafePointer(to: &addr) { aptr in
        aptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { saptr in
            connect(fd, saptr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    if connected != 0 {
        close(fd)
        return nil
    }
    return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
}

private func ensureMicrophonePermission() throws {
    // Without an explicit authorization request the engine can start while its
    // input remains silent, which looks like a successful helper launch to the
    // daemon. Check exactly once after publishing the socket/PID handshake but
    // before starting capture, so a denial becomes a terminal setup failure
    // instead of a connect/EOF/relaunch loop.
    let status = AVCaptureDevice.authorizationStatus(for: .audio)
    fputs("microphone: TCC authorization status = \(status.rawValue) "
        + "(0=notDetermined 1=restricted 2=denied 3=authorized)\n", stderr)
    if status == .notDetermined {
        var finished = false
        var granted = false
        AVCaptureDevice.requestAccess(for: .audio) { ok in
            granted = ok
            finished = true
        }
        while !finished {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
        }
        fputs("microphone: permission prompt result granted=\(granted)\n", stderr)
        if !granted {
            throw NSError(
                domain: "BlueyAudio", code: 10,
                userInfo: [NSLocalizedDescriptionKey: "microphone access denied by the user"]
            )
        }
    } else if status != .authorized {
        throw NSError(
            domain: "BlueyAudio", code: 11,
            userInfo: [NSLocalizedDescriptionKey:
                "microphone access not authorized (status \(status.rawValue)); "
                + "grant Microphone to BlueyAudio in System Settings → Privacy"]
        )
    }
}

/// Writes 16 kHz mono i16 LE PCM to a sink (stdout by default, or the daemon's
/// UNIX socket when the helper was launched as a bundle) from float input.
private final class PCM16Writer {
    private let handle: FileHandle
    private let lock = NSLock()
    private var carry: Double = 0.0

    /// Default sink is stdout (system path). The socket sink is installed by
    /// `main` when `--socket` is passed (mic path), so the same writer serves both.
    init(handle: FileHandle = FileHandle.standardOutput) {
        self.handle = handle
    }

    func writeMonoFloat(_ samples: [Float], sourceSampleRate: Double) {
        guard !samples.isEmpty, sourceSampleRate > 0 else { return }
        lock.lock()
        var output = Data()
        output.reserveCapacity(samples.count * 2)
        let ratio = 16_000.0 / sourceSampleRate
        var maxPeak: Float = 0.0
        for sample in samples {
            let absSample = abs(sample)
            if absSample > maxPeak { maxPeak = absSample }
            carry += ratio
            while carry >= 1.0 {
                let clamped = max(-1.0, min(1.0, sample.isFinite ? sample : 0.0))
                var sample = Int16(clamped * 32767.0)
                withUnsafeBytes(of: &sample) { output.append(contentsOf: $0) }
                carry -= 1.0
            }
        }
        writeAll(output)
        lock.unlock()
    }

    /// Write every byte to the sink fd, tolerating short writes. If the sink is
    /// gone (daemon disconnected → EPIPE), exit cleanly. Uses `write(2)` directly
    /// (not `FileHandle.write`, which raises on EPIPE); SIGPIPE is ignored
    /// process-wide (see `main`), so a broken pipe is an `errno`, not a crash.
    private func writeAll(_ data: Data) {
        let fd = handle.fileDescriptor
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            guard let base = raw.baseAddress else { return }
            var offset = 0
            let total = raw.count
            while offset < total {
                let n = write(fd, base + offset, total - offset)
                if n > 0 {
                    offset += n
                } else if n < 0 && errno == EINTR {
                    continue
                } else {
                    fputs("bluey audio helper: sink closed (errno \(errno)); exiting\n", stderr)
                    exit(0)
                }
            }
        }
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
    private let writer: PCM16Writer

    // Core Audio resources owned by this capture; torn down on cleanup.
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    /// The tap's real stream sample rate (often 48 kHz), read from the tap.
    private var tapSampleRate: Double = 48_000

    init(durationMs: Int, continuous: Bool, appBundleId: String?, sink: FileHandle) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
        self.appBundleId = appBundleId
        self.writer = PCM16Writer(handle: sink)
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
            let errorCode = tapErr == kAudioDevicePermissionsError
                ? systemPermissionDeniedErrorCode : 20
            throw NSError(
                domain: "BlueyAudio",
                code: errorCode,
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
    private let writer: PCM16Writer
    private let engine = AVAudioEngine()

    init(durationMs: Int, continuous: Bool, sink: FileHandle) {
        self.duration = TimeInterval(durationMs) / 1_000.0
        self.continuous = continuous
        self.writer = PCM16Writer(handle: sink)
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
        try ensureMicrophonePermission()

        let input = engine.inputNode
        // Enable AEC BEFORE reading the input format: the VoiceProcessingIO unit
        // imposes its own sample rate and channel count, so a format captured
        // beforehand would no longer describe the buffers the tap receives and
        // the converter would emit garbled audio.
        _ = enableEchoCancellation(on: input)
        let inputFormat = input.outputFormat(forBus: 0)
        fputs("microphone: input format \(inputFormat.sampleRate)Hz "
            + "\(inputFormat.channelCount)ch; tap installed\n", stderr)

        var tapFireCount = 0
        input.installTap(onBus: 0, bufferSize: 1_024, format: nil) { [weak self] buffer, _ in
            guard let self else { return }
            tapFireCount += 1
            if tapFireCount == 1 {
                fputs("microphone: FIRST tap buffer received (frames=\(buffer.frameLength), rate=\(buffer.format.sampleRate)Hz, ch=\(buffer.format.channelCount)) — audio is flowing\n", stderr)
            }
            guard buffer.frameLength > 0, let channelData = buffer.floatChannelData else { return }
            let frameLength = Int(buffer.frameLength)
            let channelCount = Int(buffer.format.channelCount)
            let sampleRate = buffer.format.sampleRate > 0 ? buffer.format.sampleRate : 48_000.0

            if channelCount == 1 {
                let samples = Array(UnsafeBufferPointer(start: channelData[0], count: frameLength))
                self.writer.writeMonoFloat(samples, sourceSampleRate: sampleRate)
            } else if channelCount > 1 {
                var mono = [Float](repeating: 0, count: frameLength)
                for c in 0..<channelCount {
                    let channelPtr = channelData[c]
                    for f in 0..<frameLength {
                        mono[f] += channelPtr[f]
                    }
                }
                let scale = 1.0 / Float(channelCount)
                for f in 0..<frameLength { mono[f] *= scale }
                self.writer.writeMonoFloat(mono, sourceSampleRate: sampleRate)
            }
        }
        engine.prepare()
        do {
            try engine.start()
            fputs("microphone: engine.start() OK isRunning=\(engine.isRunning) — waiting for tap buffers\n", stderr)
        } catch {
            fputs("microphone: engine.start() FAILED: \(error.localizedDescription)\n", stderr)
            throw error
        }

        if continuous {
            // Park on the CURRENT thread's run loop to keep the process alive.
            // This method is invoked on the MAIN thread (see main.swift) so this
            // services the main run loop AVAudioEngine needs. (Previously this was
            // reached from inside a background `Task`, where the engine started on
            // a thread with no live run loop and the tap never fired — the
            // "engine started, 0 buffers" bug.)
            RunLoop.current.run()
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

    if args.launchProbe {
        return completeLaunchProbe(outputPath: args.launchProbeOutput)
    }

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

    // Resolve the PCM sink: the daemon's UNIX socket when launched as a bundle
    // (`--socket`, the mic path), else stdout (the system path). When `--socket`
    // is given but the connection fails, exit — the daemon reads the socket, not
    // our stdout, so a stdout fallback would silently drop all audio.
    let sink: FileHandle
    if let socketPath = args.socketPath {
        guard let connected = connectSocket(socketPath) else {
            fputs("bluey audio helper: could not connect to socket \(socketPath)\n", stderr)
            return 4
        }
        sink = connected
    } else {
        sink = FileHandle.standardOutput
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
                appBundleId: args.appBundleId,
                sink: sink
            )
            try await capture.run()
        case .microphone:
            let capture = MicrophoneCapture(
                durationMs: args.durationMs,
                continuous: args.continuous,
                sink: sink
            )
            try capture.run()
        }
        return 0
    } catch {
        fputs("bluey audio helper failed: \(error.localizedDescription)\n", stderr)
        return exitCode(for: error, source: args.source)
    }
}

// The MICROPHONE path must run its AVAudioEngine on the MAIN thread: the engine
// attaches to the calling thread's run loop, and starting it on a background
// `Task` thread (which has no live run loop) leaves the input tap silent — the
// engine "starts" but 0 buffers ever arrive. So we parse args up front and, for
// mic mode, run synchronously on the main thread here (its own RunLoop.run()
// keeps the process alive). Every other source keeps the async Task path.
// Wrapped in a function so no top-level constant exposes the private `Args`
// type (Swift rejects a file-scope `let` whose type is private).
private func runMicrophoneOnMainThread() {
    let parsed = parseArgs()

    // Publish the helper PID before connecting so the daemon can terminate this
    // detached LaunchServices process even if shutdown races socket setup or a
    // first-use permission prompt.
    writeHelperStatus("starting", to: parsed.statusPath)

    let sink: FileHandle
    if let socketPath = parsed.socketPath {
        guard let connected = connectSocket(socketPath) else {
            fputs("bluey audio helper: could not connect to socket \(socketPath)\n", stderr)
            exit(4)
        }
        sink = connected
    } else {
        sink = FileHandle.standardOutput
    }

    // Connect to the daemon before macOS presents a first-use prompt. The
    // daemon can then keep one stable helper/session alive while the user
    // decides, and an explicit sidecar state distinguishes denial from an AMFI,
    // LaunchServices, architecture, or engine setup failure.
    writeHelperStatus("permission_checking", to: parsed.statusPath)
    do {
        try ensureMicrophonePermission()
        writeHelperStatus("authorized", to: parsed.statusPath)
    } catch {
        let code = exitCode(for: error, source: .microphone)
        writeHelperStatus(
            code == permissionDeniedExitCode ? "permission_denied" : "failed",
            to: parsed.statusPath
        )
        fputs("bluey audio helper failed: \(error.localizedDescription)\n", stderr)
        exit(code)
    }

    do {
        let capture = MicrophoneCapture(
            durationMs: parsed.durationMs,
            continuous: parsed.continuous,
            sink: sink
        )
        try capture.run() // continuous mode parks on the main run loop inside
        writeHelperStatus("stopped", to: parsed.statusPath)
        exit(0)
    } catch {
        writeHelperStatus("failed", to: parsed.statusPath)
        fputs("bluey audio helper failed: \(error.localizedDescription)\n", stderr)
        exit(exitCode(for: error, source: .microphone))
    }
}

// Ignore SIGPIPE for every mode, including the synchronous microphone branch.
// A daemon socket close should surface EPIPE to PCM16Writer instead of killing
// the detached helper before it can write its final status.
signal(SIGPIPE, SIG_IGN)

if CommandLine.arguments.contains("microphone") {
    runMicrophoneOnMainThread()
} else {
    Task {
        exit(await run())
    }
    dispatchMain()
}
