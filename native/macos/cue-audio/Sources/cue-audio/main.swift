import AudioBridge
import AVFoundation
import CoreMedia
import CueAudioCore
import Darwin
import Dispatch
import Foundation
import ScreenCaptureKit

private let helperProtocolVersion = 1
private let targetSampleRate = 16_000
private let workerStopTimeout = DispatchTimeInterval.seconds(1)
private let completionPollNanoseconds: UInt64 = 50_000_000

private enum NativeCaptureFailure: Error {
    case allocation
    case captureStopped
    case noDisplay
    case noMicrophone
    case outputConfiguration
    case processor
    case stdoutWrite
    case workerTimeout
}

private enum OutputWriteResult {
    case success
    case closed
    case failed
}

private func emitDiagnostic(_ payload: [String: Any]) {
    guard
        JSONSerialization.isValidJSONObject(payload),
        var data = try? JSONSerialization.data(withJSONObject: payload)
    else {
        return
    }
    data.append(0x0A)
    FileHandle.standardError.write(data)
}

private func emitReady(source: CaptureSource, backend: String) {
    emitDiagnostic([
        "event": "ready",
        "protocol_version": helperProtocolVersion,
        "source": source.rawValue,
        "backend": backend,
        "format": [
            "sample_rate_hz": targetSampleRate,
            "channel_count": 1,
            "sample_format": "i16_le",
        ],
    ])
}

private func emitWarning(
    source: CaptureSource,
    code: String,
    operation: String,
    count: UInt64
) {
    emitDiagnostic([
        "event": "warning",
        "protocol_version": helperProtocolVersion,
        "source": source.rawValue,
        "code": code,
        "operation": operation,
        "count": count,
    ])
}

private func emitError(
    source: CaptureSource,
    code: String,
    operation: String,
    recoverable: Bool
) {
    emitDiagnostic([
        "event": "error",
        "protocol_version": helperProtocolVersion,
        "source": source.rawValue,
        "code": code,
        "operation": operation,
        "recoverable": recoverable,
    ])
}

private func emitStopped(
    source: CaptureSource,
    reason: String,
    exitCode: Int32
) {
    emitDiagnostic([
        "event": "stopped",
        "protocol_version": helperProtocolVersion,
        "source": source.rawValue,
        "reason": reason,
        "exit_code": exitCode,
    ])
}

private func configureOutput() -> Bool {
    _ = Darwin.signal(SIGPIPE, SIG_IGN)
    let flags = fcntl(STDOUT_FILENO, F_GETFL)
    guard flags >= 0 else {
        return false
    }
    return fcntl(STDOUT_FILENO, F_SETFL, flags | O_NONBLOCK) == 0
}

private func writeOutput(
    _ samples: UnsafeBufferPointer<Int16>,
    bridge: OpaquePointer
) -> OutputWriteResult {
    guard let baseAddress = samples.baseAddress, !samples.isEmpty else {
        return .success
    }

    let bytes = UnsafeRawPointer(baseAddress).assumingMemoryBound(to: UInt8.self)
    let byteCount = samples.count * MemoryLayout<Int16>.size
    var offset = 0
    var blockedPolls = 0

    while offset < byteCount {
        let written = Darwin.write(
            STDOUT_FILENO,
            bytes.advanced(by: offset),
            byteCount - offset
        )
        if written > 0 {
            offset += written
            blockedPolls = 0
            continue
        }
        if written == 0 {
            return .closed
        }

        switch errno {
        case EINTR:
            continue
        case EPIPE:
            return .closed
        case EAGAIN:
            var descriptor = pollfd(
                fd: STDOUT_FILENO,
                events: Int16(POLLOUT),
                revents: 0
            )
            let pollResult = Darwin.poll(&descriptor, 1, 100)
            if pollResult < 0, errno != EINTR {
                return .failed
            }
            blockedPolls += 1
            let stopLimit = bluey_audio_bridge_stop_requested(bridge) ? 1 : 5
            if blockedPolls >= stopLimit {
                return .failed
            }
        default:
            return .failed
        }
    }
    return .success
}

private final class AudioPipeline: @unchecked Sendable {
    private let bridge: OpaquePointer
    private let processor: OpaquePointer
    private let wake = DispatchSemaphore(value: 0)
    private let worker = DispatchQueue(
        label: "sh.bluey.audio.convert",
        qos: .userInitiated
    )
    private let workerGroup = DispatchGroup()
    private var workerStarted = false

    init() throws {
        guard let bridge = bluey_audio_bridge_create() else {
            throw NativeCaptureFailure.allocation
        }
        guard let processor = bluey_audio_processor_create() else {
            bluey_audio_bridge_destroy(bridge)
            throw NativeCaptureFailure.allocation
        }
        self.bridge = bridge
        self.processor = processor
    }

    deinit {
        requestStop()
        if workerStarted {
            _ = workerGroup.wait(timeout: .now() + workerStopTimeout)
        }
        bluey_audio_processor_destroy(processor)
        bluey_audio_bridge_destroy(bridge)
    }

    func startWorker() {
        precondition(!workerStarted)
        workerStarted = true
        workerGroup.enter()
        worker.async { [self] in
            defer { workerGroup.leave() }
            runWorker()
        }
    }

    func enqueue(_ sampleBuffer: CMSampleBuffer) {
        if bluey_audio_bridge_push_sample_buffer(bridge, sampleBuffer) > 0 {
            wake.signal()
        }
    }

    func enqueue(_ buffer: AVAudioPCMBuffer) {
        let streamDescription = buffer.format.streamDescription
        if bluey_audio_bridge_push_audio_buffer_list(
            bridge,
            buffer.audioBufferList,
            UInt32(buffer.frameLength),
            streamDescription
        ) > 0 {
            wake.signal()
        }
    }

    func reportCaptureFailure(_ code: Int32) {
        guard !stopRequested else {
            return
        }
        bluey_audio_bridge_report_capture_failure(bridge, code)
        wake.signal()
    }

    func requestStop() {
        bluey_audio_bridge_request_stop(bridge)
        wake.signal()
    }

    func stopWorker() -> Bool {
        requestStop()
        guard workerStarted else {
            return true
        }
        return workerGroup.wait(
            timeout: .now() + workerStopTimeout
        ) == .success
    }

    var stopRequested: Bool {
        bluey_audio_bridge_stop_requested(bridge)
    }

    var outputClosed: Bool {
        bluey_audio_bridge_output_closed(bridge)
    }

    var captureFailure: Int32 {
        bluey_audio_bridge_capture_failure(bridge)
    }

    var workerFailure: Int32 {
        bluey_audio_bridge_worker_failure(bridge)
    }

    func takeDroppedPackets() -> UInt64 {
        bluey_audio_bridge_take_dropped_packets(bridge)
    }

    func takeInvalidPackets() -> UInt64 {
        bluey_audio_bridge_take_invalid_packets(bridge)
    }

    private func runWorker() {
        var output = [Int16](
            repeating: 0,
            count: bluey_audio_bridge_max_output_samples()
        )

        while true {
            var processedPacket = false
            while true {
                var outputCount = 0
                let result = output.withUnsafeMutableBufferPointer { samples in
                    bluey_audio_processor_process_next(
                        bridge,
                        processor,
                        samples.baseAddress,
                        samples.count,
                        &outputCount
                    )
                }

                switch result {
                case BLUEY_AUDIO_PROCESS_EMPTY:
                    break
                case BLUEY_AUDIO_PROCESS_OK:
                    processedPacket = true
                    if outputCount > 0 {
                        let writeResult = output.withUnsafeBufferPointer { samples in
                            writeOutput(
                                UnsafeBufferPointer(
                                    start: samples.baseAddress,
                                    count: outputCount
                                ),
                                bridge: bridge
                            )
                        }
                        switch writeResult {
                        case .success:
                            break
                        case .closed:
                            bluey_audio_bridge_report_output_closed(bridge)
                            return
                        case .failed:
                            bluey_audio_bridge_report_worker_failure(
                                bridge,
                                BLUEY_AUDIO_WORKER_FAILURE_STDOUT
                            )
                            return
                        }
                    }
                    continue
                case BLUEY_AUDIO_PROCESS_INVALID,
                     BLUEY_AUDIO_PROCESS_OUTPUT_FULL:
                    bluey_audio_bridge_report_worker_failure(
                        bridge,
                        BLUEY_AUDIO_WORKER_FAILURE_PROCESSOR
                    )
                    return
                default:
                    bluey_audio_bridge_report_worker_failure(
                        bridge,
                        BLUEY_AUDIO_WORKER_FAILURE_PROCESSOR
                    )
                    return
                }
                break
            }

            if stopRequested, bluey_audio_bridge_is_empty(bridge) {
                return
            }
            if !processedPacket {
                _ = wake.wait(timeout: .now() + .milliseconds(100))
            }
        }
    }
}

private struct BoundedWarningCounter {
    private(set) var total: UInt64 = 0
    private var nextReport: UInt64 = 1

    mutating func add(_ delta: UInt64) -> UInt64? {
        guard delta > 0 else {
            return nil
        }
        let addition = total.addingReportingOverflow(delta)
        total = addition.overflow ? UInt64.max : addition.partialValue
        guard total >= nextReport else {
            return nil
        }
        while nextReport <= total, nextReport <= UInt64.max / 2 {
            nextReport *= 2
        }
        return total
    }
}

private struct PipelineWarningReporter {
    private var dropped = BoundedWarningCounter()
    private var invalid = BoundedWarningCounter()

    mutating func poll(source: CaptureSource, pipeline: AudioPipeline) {
        if let count = dropped.add(pipeline.takeDroppedPackets()) {
            emitWarning(
                source: source,
                code: "callback_queue_overflow",
                operation: "enqueue_audio_packet",
                count: count
            )
        }
        if let count = invalid.add(pipeline.takeInvalidPackets()) {
            emitWarning(
                source: source,
                code: "invalid_audio_packet",
                operation: "validate_audio_packet",
                count: count
            )
        }
    }
}

private func waitForCompletion(
    source: CaptureSource,
    durationMs: UInt32,
    continuous: Bool,
    pipeline: AudioPipeline,
    warnings: inout PipelineWarningReporter
) async throws -> String {
    let startedAt = DispatchTime.now().uptimeNanoseconds
    let durationNanoseconds = UInt64(durationMs) * 1_000_000

    while true {
        warnings.poll(source: source, pipeline: pipeline)

        if pipeline.captureFailure != 0 {
            throw NativeCaptureFailure.captureStopped
        }
        let workerFailure = pipeline.workerFailure
        if workerFailure == Int32(BLUEY_AUDIO_WORKER_FAILURE_STDOUT.rawValue) {
            throw NativeCaptureFailure.stdoutWrite
        }
        if workerFailure == Int32(BLUEY_AUDIO_WORKER_FAILURE_PROCESSOR.rawValue) {
            throw NativeCaptureFailure.processor
        }
        if workerFailure == Int32(BLUEY_AUDIO_WORKER_FAILURE_TIMEOUT.rawValue) {
            throw NativeCaptureFailure.workerTimeout
        }
        if workerFailure != Int32(BLUEY_AUDIO_WORKER_FAILURE_NONE.rawValue) {
            throw NativeCaptureFailure.processor
        }
        if pipeline.outputClosed {
            return "stdout_closed"
        }
        if pipeline.stopRequested {
            throw NativeCaptureFailure.captureStopped
        }
        if !continuous {
            let elapsed = DispatchTime.now().uptimeNanoseconds - startedAt
            if elapsed >= durationNanoseconds {
                return "duration_complete"
            }
        }
        try await Task.sleep(nanoseconds: completionPollNanoseconds)
    }
}

@available(macOS 13.0, *)
private final class SystemAudioCapture: NSObject, SCStreamOutput, SCStreamDelegate {
    private let durationMs: UInt32
    private let continuous: Bool
    private let pipeline: AudioPipeline
    private var stream: SCStream?

    init(durationMs: UInt32, continuous: Bool) throws {
        self.durationMs = durationMs
        self.continuous = continuous
        pipeline = try AudioPipeline()
        super.init()
    }

    func run() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: false
        )
        guard let display = content.displays.first else {
            throw NativeCaptureFailure.noDisplay
        }

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let configuration = SCStreamConfiguration()
        configuration.capturesAudio = true
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 48_000
        configuration.channelCount = 1
        configuration.width = 2
        configuration.height = 2
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        configuration.queueDepth = 3

        let stream = SCStream(
            filter: filter,
            configuration: configuration,
            delegate: self
        )
        self.stream = stream
        let callbackQueue = DispatchQueue(
            label: "sh.bluey.audio.system",
            qos: .userInitiated
        )
        try stream.addStreamOutput(
            self,
            type: .audio,
            sampleHandlerQueue: callbackQueue
        )
        try await stream.startCapture()

        emitReady(source: .system, backend: "screen_capture_kit")
        pipeline.startWorker()
        var warnings = PipelineWarningReporter()

        do {
            let reason = try await waitForCompletion(
                source: .system,
                durationMs: durationMs,
                continuous: continuous,
                pipeline: pipeline,
                warnings: &warnings
            )
            pipeline.requestStop()
            try await stream.stopCapture()
            guard pipeline.stopWorker() else {
                throw NativeCaptureFailure.workerTimeout
            }
            warnings.poll(source: .system, pipeline: pipeline)
            emitStopped(source: .system, reason: reason, exitCode: 0)
        } catch {
            pipeline.requestStop()
            try? await stream.stopCapture()
            _ = pipeline.stopWorker()
            warnings.poll(source: .system, pipeline: pipeline)
            throw error
        }
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of type: SCStreamOutputType
    ) {
        guard type == .audio else {
            return
        }
        pipeline.enqueue(sampleBuffer)
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        pipeline.reportCaptureFailure(1)
    }
}

private final class MicrophoneCapture {
    private let durationMs: UInt32
    private let continuous: Bool
    private let pipeline: AudioPipeline
    private let engine = AVAudioEngine()

    init(durationMs: UInt32, continuous: Bool) throws {
        self.durationMs = durationMs
        self.continuous = continuous
        pipeline = try AudioPipeline()
    }

    func run() async throws {
        try await ensureMicrophoneAccess()
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard inputFormat.channelCount > 0 else {
            throw NativeCaptureFailure.noMicrophone
        }

        input.installTap(
            onBus: 0,
            bufferSize: 1_024,
            format: inputFormat
        ) { [pipeline] buffer, _ in
            pipeline.enqueue(buffer)
        }

        do {
            try engine.start()
            emitReady(source: .microphone, backend: "av_audio_engine")
            pipeline.startWorker()
            var warnings = PipelineWarningReporter()
            let reason = try await waitForCompletion(
                source: .microphone,
                durationMs: durationMs,
                continuous: continuous,
                pipeline: pipeline,
                warnings: &warnings
            )
            pipeline.requestStop()
            engine.stop()
            input.removeTap(onBus: 0)
            guard pipeline.stopWorker() else {
                throw NativeCaptureFailure.workerTimeout
            }
            warnings.poll(source: .microphone, pipeline: pipeline)
            emitStopped(source: .microphone, reason: reason, exitCode: 0)
        } catch {
            pipeline.requestStop()
            engine.stop()
            input.removeTap(onBus: 0)
            _ = pipeline.stopWorker()
            throw error
        }
    }

    private func ensureMicrophoneAccess() async throws {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return
        case .notDetermined:
            let granted = await withCheckedContinuation { continuation in
                AVCaptureDevice.requestAccess(for: .audio) { allowed in
                    continuation.resume(returning: allowed)
                }
            }
            if granted {
                return
            }
            fallthrough
        case .denied, .restricted:
            throw NSError(
                domain: "BlueyAudio",
                code: 4,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "microphone permission is not granted",
                ]
            )
        @unknown default:
            throw NSError(
                domain: "BlueyAudio",
                code: 5,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "microphone permission status is unknown",
                ]
            )
        }
    }
}

private func emitFailure(source: CaptureSource, error: Error) -> Int32 {
    let nsError = error as NSError
    let normalized = error.localizedDescription.lowercased()
    let permissionDenied = (nsError.domain == "BlueyAudio"
        && [4, 5].contains(nsError.code))
        || normalized.contains("permission")
        || normalized.contains("not authorized")
        || normalized.contains("access denied")

    if permissionDenied {
        emitError(
            source: source,
            code: "permission_denied",
            operation: source == .system
                ? "start_screen_capture"
                : "request_microphone_access",
            recoverable: false
        )
        emitStopped(
            source: source,
            reason: "permission_denied",
            exitCode: 3
        )
        return 3
    }

    let code: String
    let operation: String
    let recoverable: Bool
    switch error {
    case NativeCaptureFailure.allocation:
        code = "allocation_failed"
        operation = "initialize_audio_pipeline"
        recoverable = false
    case NativeCaptureFailure.captureStopped:
        code = "device_invalidated"
        operation = "capture_stream"
        recoverable = true
    case NativeCaptureFailure.noDisplay:
        code = "no_display"
        operation = "discover_screen_content"
        recoverable = false
    case NativeCaptureFailure.noMicrophone:
        code = "no_input_device"
        operation = "configure_microphone"
        recoverable = true
    case NativeCaptureFailure.outputConfiguration:
        code = "stdout_configuration_failed"
        operation = "configure_stdout"
        recoverable = false
    case NativeCaptureFailure.processor:
        code = "audio_processor_failed"
        operation = "downmix_resample"
        recoverable = false
    case NativeCaptureFailure.stdoutWrite:
        code = "stdout_write_failed"
        operation = "write_pcm"
        recoverable = true
    case NativeCaptureFailure.workerTimeout:
        code = "worker_stop_timeout"
        operation = "stop_audio_worker"
        recoverable = true
    default:
        code = "native_capture_failed"
        operation = source == .system
            ? "screen_capture_kit"
            : "av_audio_engine"
        recoverable = false
    }

    emitError(
        source: source,
        code: code,
        operation: operation,
        recoverable: recoverable
    )
    emitStopped(source: source, reason: "capture_error", exitCode: 1)
    return 1
}

private func run() async -> Int32 {
    let arguments: CaptureArguments
    do {
        arguments = try parseCaptureArguments(
            Array(CommandLine.arguments.dropFirst())
        )
    } catch let error as CaptureArgumentError {
        emitError(
            source: .system,
            code: error.rawValue,
            operation: "parse_arguments",
            recoverable: false
        )
        emitStopped(source: .system, reason: "argument_error", exitCode: 2)
        return 2
    } catch {
        emitError(
            source: .system,
            code: "unknown_option",
            operation: "parse_arguments",
            recoverable: false
        )
        emitStopped(source: .system, reason: "argument_error", exitCode: 2)
        return 2
    }

    guard configureOutput() else {
        return emitFailure(
            source: arguments.source,
            error: NativeCaptureFailure.outputConfiguration
        )
    }

    do {
        switch arguments.source {
        case .system:
            guard #available(macOS 13.0, *) else {
                emitError(
                    source: .system,
                    code: "unsupported_os",
                    operation: "initialize_screen_capture_kit",
                    recoverable: false
                )
                emitStopped(
                    source: .system,
                    reason: "unsupported_os",
                    exitCode: 2
                )
                return 2
            }
            let capture = try SystemAudioCapture(
                durationMs: arguments.durationMs,
                continuous: arguments.continuous
            )
            try await capture.run()

        case .microphone:
            let capture = try MicrophoneCapture(
                durationMs: arguments.durationMs,
                continuous: arguments.continuous
            )
            try await capture.run()
        }
        return 0
    } catch {
        return emitFailure(source: arguments.source, error: error)
    }
}

Task {
    exit(await run())
}
dispatchMain()
