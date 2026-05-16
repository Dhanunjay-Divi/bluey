/// cue-whisper: Local Whisper STT helper for macOS.
///
/// Reads PCM16 LE 16kHz mono from stdin in ~3-second chunks,
/// runs whisper.cpp transcription, and emits NDJSON to stdout:
///   {"type":"partial","text":"..."}
///   {"type":"final","text":"...","confidence":0.95}

import Foundation
import SwiftWhisper
import whisper_cpp

let sampleRate = 16000
let chunkDurationSec = 3
let chunkSamples = sampleRate * chunkDurationSec
let chunkBytes = chunkSamples * 2

func log(_ msg: String) {
    FileHandle.standardError.write("cue-whisper: \(msg)\n".data(using: .utf8)!)
}

func emit(_ json: String) {
    FileHandle.standardOutput.write((json + "\n").data(using: .utf8)!)
}

let modelPath: String = {
    if let env = ProcessInfo.processInfo.environment["BLUEY_WHISPER_MODEL"] {
        return env
    }
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    return "\(home)/.cache/bluey/whisper/tiny.en-q5_1.bin"
}()

guard FileManager.default.fileExists(atPath: modelPath) else {
    log("ERROR: model not found at \(modelPath)")
    log("Run: infra/scripts/download-whisper-model.sh")
    exit(1)
}

log("Loading model: \(modelPath)")
guard let ctx = whisper_init_from_file(modelPath) else {
    log("ERROR: failed to initialize whisper context")
    exit(1)
}
log("Model loaded, reading PCM16 16kHz mono from stdin...")

var params = whisper_full_default_params(WHISPER_SAMPLING_GREEDY)
params.print_realtime = false
params.print_progress = false
params.print_timestamps = false
params.print_special = false
params.translate = false
params.no_context = true
params.single_segment = false
params.n_threads = Int32(max(1, min(8, ProcessInfo.processInfo.processorCount - 2)))
let langStr = strdup("en")
params.language = UnsafePointer(langStr)

let stdinHandle = FileHandle.standardInput

while true {
    let data = stdinHandle.readData(ofLength: chunkBytes)
    if data.isEmpty { break }

    let floats: [Float] = data.withUnsafeBytes { (buf: UnsafeRawBufferPointer) in
        let count = buf.count / 2
        return (0..<count).map { i in
            Float(buf.loadUnaligned(fromByteOffset: i * 2, as: Int16.self)) / 32768.0
        }
    }

    // Skip silence (RMS threshold)
    let rms = sqrt(floats.reduce(0.0) { $0 + $1 * $1 } / max(Float(floats.count), 1.0))
    if rms < 0.01 { continue }

    emit("{\"type\":\"partial\",\"text\":\"[transcribing...]\"}")

    let ret = floats.withUnsafeBufferPointer { buf in
        whisper_full(ctx, params, buf.baseAddress, Int32(buf.count))
    }

    if ret != 0 {
        log("whisper_full failed with code \(ret)")
        continue
    }

    let nSegments = whisper_full_n_segments(ctx)
    for i in 0..<nSegments {
        guard let cStr = whisper_full_get_segment_text(ctx, i) else { continue }
        let text = String(cString: cStr).trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { continue }

        let nTokens = whisper_full_n_tokens(ctx, i)
        var sumP: Float = 0
        for t in 0..<nTokens {
            sumP += whisper_full_get_token_p(ctx, i, t)
        }
        let confidence = nTokens > 0 ? sumP / Float(nTokens) : 0.5

        let escaped = text
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
        emit("{\"type\":\"final\",\"text\":\"\(escaped)\",\"confidence\":\(String(format: "%.2f", confidence))}")
    }
}

whisper_free(ctx)
free(langStr)
