import AudioBridge
import Darwin
import Foundation

private enum TestFailure: Error {
    case check(String)
    case processing(BlueyAudioProcessResult)
}

private func require(
    _ condition: @autoclosure () throws -> Bool,
    _ message: String
) throws {
    if try !condition() {
        throw TestFailure.check(message)
    }
}

private func testRingBounds() throws {
    guard let bridge = bluey_audio_bridge_create() else {
        throw TestFailure.check("bridge allocation failed")
    }
    defer { bluey_audio_bridge_destroy(bridge) }

    let frames = Int(bluey_audio_bridge_max_packet_frames())
    let input = [Float](repeating: 0.25, count: frames)
    let capacity = Int(bluey_audio_bridge_capacity())
    try input.withUnsafeBufferPointer { samples in
        for _ in 0..<capacity {
            try require(
                bluey_audio_bridge_push_interleaved_f32(
                    bridge,
                    samples.baseAddress,
                    UInt32(frames),
                    1,
                    48_000
                ) == 1,
                "ring rejected an in-capacity packet"
            )
        }
        try require(
            bluey_audio_bridge_push_interleaved_f32(
                bridge,
                samples.baseAddress,
                UInt32(frames),
                1,
                48_000
            ) == 0,
            "ring accepted an over-capacity packet"
        )
    }
    try require(
        bluey_audio_bridge_take_dropped_packets(bridge) == 1,
        "ring did not count overflow exactly"
    )
    try require(
        bluey_audio_bridge_take_dropped_packets(bridge) == 0,
        "overflow counter did not reset"
    )
}

private func testInvalidAndStop() throws {
    guard let bridge = bluey_audio_bridge_create() else {
        throw TestFailure.check("bridge allocation failed")
    }
    defer { bluey_audio_bridge_destroy(bridge) }

    let input = [Float](repeating: 0, count: 128)
    try input.withUnsafeBufferPointer { samples in
        try require(
            bluey_audio_bridge_push_interleaved_f32(
                bridge,
                samples.baseAddress,
                UInt32(input.count),
                1,
                1_000
            ) == 0,
            "invalid source rate was accepted"
        )
    }
    try require(
        bluey_audio_bridge_take_invalid_packets(bridge) == 1,
        "invalid packet was not counted"
    )

    bluey_audio_bridge_request_stop(bridge)
    try require(
        bluey_audio_bridge_stop_requested(bridge),
        "stop request was not visible"
    )
    try input.withUnsafeBufferPointer { samples in
        try require(
            bluey_audio_bridge_push_interleaved_f32(
                bridge,
                samples.baseAddress,
                UInt32(input.count),
                1,
                48_000
            ) == 0,
            "packet was accepted after stop"
        )
    }
}

private func renderTone(
    sampleRate: Double,
    frequency: Double
) throws -> [Int16] {
    guard
        let bridge = bluey_audio_bridge_create(),
        let processor = bluey_audio_processor_create()
    else {
        throw TestFailure.check("audio pipeline allocation failed")
    }
    defer {
        bluey_audio_processor_destroy(processor)
        bluey_audio_bridge_destroy(bridge)
    }

    let inputCount = Int(sampleRate)
    let input = (0..<inputCount).map { index in
        Float(
            0.5 * sin(
                2.0 * Double.pi * frequency * Double(index) / sampleRate
            )
        )
    }
    var output = [Int16](
        repeating: 0,
        count: bluey_audio_bridge_max_output_samples()
    )
    var rendered = [Int16]()
    rendered.reserveCapacity(16_000)

    try input.withUnsafeBufferPointer { samples in
        var offset = 0
        while offset < input.count {
            let count = min(
                Int(bluey_audio_bridge_max_packet_frames()),
                input.count - offset
            )
            try require(
                bluey_audio_bridge_push_interleaved_f32(
                    bridge,
                    samples.baseAddress?.advanced(by: offset),
                    UInt32(count),
                    1,
                    sampleRate
                ) == 1,
                "valid tone packet was not enqueued"
            )

            var outputCount = 0
            let result = output.withUnsafeMutableBufferPointer { outputSamples in
                bluey_audio_processor_process_next(
                    bridge,
                    processor,
                    outputSamples.baseAddress,
                    outputSamples.count,
                    &outputCount
                )
            }
            guard result == BLUEY_AUDIO_PROCESS_OK else {
                throw TestFailure.processing(result)
            }
            rendered.append(contentsOf: output[..<outputCount])
            offset += count
        }
    }
    try require(
        bluey_audio_bridge_is_empty(bridge),
        "processor left a packet in the ring"
    )
    return rendered
}

private func rms(_ samples: [Int16]) -> Double {
    let sum = samples.reduce(0.0) { partial, sample in
        let normalized = Double(sample) / 32_768.0
        return partial + normalized * normalized
    }
    return samples.isEmpty ? 0 : sqrt(sum / Double(samples.count))
}

private func fnv1a(_ samples: [Int16]) -> UInt64 {
    var hash: UInt64 = 1_469_598_103_934_665_603
    for sample in samples {
        let bits = UInt16(bitPattern: sample)
        hash ^= UInt64(bits & 0xFF)
        hash &*= 1_099_511_628_211
        hash ^= UInt64(bits >> 8)
        hash &*= 1_099_511_628_211
    }
    return hash
}

private func testResampler() throws {
    let low48 = try renderTone(sampleRate: 48_000, frequency: 1_000)
    let low48Repeat = try renderTone(sampleRate: 48_000, frequency: 1_000)
    let low44 = try renderTone(sampleRate: 44_100, frequency: 1_000)
    let high48 = try renderTone(sampleRate: 48_000, frequency: 12_000)

    try require(
        (15_900...16_000).contains(low48.count),
        "48 kHz output count was \(low48.count)"
    )
    try require(
        (15_900...16_000).contains(low44.count),
        "44.1 kHz output count was \(low44.count)"
    )
    try require(
        low48.count == low48Repeat.count && fnv1a(low48) == fnv1a(low48Repeat),
        "resampler output was not deterministic"
    )
    let low48RMS = rms(low48)
    let low44RMS = rms(low44)
    let high48RMS = rms(high48)
    try require(
        (0.32...0.38).contains(low48RMS),
        "48 kHz passband RMS was \(low48RMS)"
    )
    try require(
        (0.32...0.38).contains(low44RMS),
        "44.1 kHz passband RMS was \(low44RMS)"
    )
    try require(
        high48RMS < low48RMS * 0.05,
        "stopband RMS was \(high48RMS)"
    )
}

private func testStereoDownmix() throws {
    guard
        let bridge = bluey_audio_bridge_create(),
        let processor = bluey_audio_processor_create()
    else {
        throw TestFailure.check("audio pipeline allocation failed")
    }
    defer {
        bluey_audio_processor_destroy(processor)
        bluey_audio_bridge_destroy(bridge)
    }

    var stereo = [Float]()
    stereo.reserveCapacity(4_096 * 2)
    for _ in 0..<4_096 {
        stereo.append(0.75)
        stereo.append(-0.75)
    }
    try stereo.withUnsafeBufferPointer { samples in
        try require(
            bluey_audio_bridge_push_interleaved_f32(
                bridge,
                samples.baseAddress,
                4_096,
                2,
                48_000
            ) == 1,
            "stereo packet was not enqueued"
        )
    }

    var output = [Int16](
        repeating: 0,
        count: bluey_audio_bridge_max_output_samples()
    )
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
    try require(result == BLUEY_AUDIO_PROCESS_OK, "stereo packet failed")
    try require(outputCount > 0, "stereo packet produced no output")
    try require(
        output[..<outputCount].allSatisfy { $0 == 0 },
        "opposite stereo channels did not cancel"
    )
}

do {
    try testRingBounds()
    try testInvalidAndStop()
    try testResampler()
    try testStereoDownmix()
    print("cue-audio bridge tests passed")
} catch {
    fputs("cue-audio bridge tests failed: \(error)\n", stderr)
    exit(1)
}
