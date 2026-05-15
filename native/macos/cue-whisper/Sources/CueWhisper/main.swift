/// cue-whisper: Local Whisper STT helper for macOS.
///
/// Reads PCM16 LE 16kHz mono from stdin in ~3-second chunks,
/// runs whisper.cpp transcription, and emits NDJSON events to stdout:
///   {"type":"partial","text":"..."}
///   {"type":"final","text":"...","confidence":0.91}
///
/// NOTE: This is a STUB implementation. Real whisper.cpp integration is
/// deferred to a follow-up. Currently detects non-silent audio and emits
/// placeholder transcripts to validate the IPC protocol.

import Foundation

let sampleRate = 16000
let chunkDurationSec = 3
let chunkSamples = sampleRate * chunkDurationSec
let chunkBytes = chunkSamples * 2

func writeEvent(_ json: String) {
    let line = json + "\n"
    guard let data = line.data(using: .utf8) else { return }
    FileHandle.standardOutput.write(data)
}

let stdin = FileHandle.standardInput
while true {
    let data = stdin.readData(ofLength: chunkBytes)
    if data.isEmpty { break }

    let samples = data.withUnsafeBytes { buf -> [Int16] in
        guard let ptr = buf.baseAddress?.assumingMemoryBound(to: Int16.self) else { return [] }
        return Array(UnsafeBufferPointer(start: ptr, count: data.count / 2))
    }

    let rms = sqrt(Double(samples.reduce(Int64(0)) { $0 + Int64($1) * Int64($1) }) / max(Double(samples.count), 1.0))

    if rms > 500 {
        writeEvent("{\"type\":\"partial\",\"text\":\"[speech detected]\"}")
        writeEvent("{\"type\":\"final\",\"text\":\"[stub transcription]\",\"confidence\":0.0}")
    }
}
