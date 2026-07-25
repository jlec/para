import CoreML
import FluidAudio
import Foundation

/// Real errors surfaced across the C boundary as a negative return code —
/// the caller (Rust) gets an error message via `para_bridge_last_error`.
private enum BridgeError: Error, CustomStringConvertible {
    case notInitialized
    case noResult
    case invalidUTF8Path

    var description: String {
        switch self {
        case .notInitialized: return "ASR models are not loaded"
        case .noResult: return "transcription produced no result"
        case .invalidUTF8Path: return "path is not valid UTF-8"
        }
    }
}

/// One bridge instance owns one loaded TDT model + its decoder state.
/// Deliberately not `Sendable`-checked across the C boundary — every call is
/// funneled through a `DispatchSemaphore`-blocked `Task`, so at most one
/// FluidAudio call is in flight per bridge instance at a time (mirrors
/// fluidaudio-rs's own bridge, which uses the same pattern).
private final class BridgeState {
    var manager: AsrManager?
    var lastError: String = ""
}

@_cdecl("para_bridge_create")
public func para_bridge_create() -> UnsafeMutableRawPointer? {
    let state = BridgeState()
    return Unmanaged.passRetained(state).toOpaque()
}

@_cdecl("para_bridge_destroy")
public func para_bridge_destroy(_ ptr: UnsafeMutableRawPointer?) {
    guard let ptr else { return }
    Unmanaged<BridgeState>.fromOpaque(ptr).release()
}

private func withState<T>(_ ptr: UnsafeMutableRawPointer?, _ body: (BridgeState) -> T) -> T? {
    guard let ptr else { return nil }
    let state = Unmanaged<BridgeState>.fromOpaque(ptr).takeUnretainedValue()
    return body(state)
}

private func cString(_ s: String) -> UnsafeMutablePointer<CChar> {
    let utf8 = Array(s.utf8CString)
    let buf = UnsafeMutablePointer<CChar>.allocate(capacity: utf8.count)
    buf.update(from: utf8, count: utf8.count)
    return buf
}

/// `AsrModelVersion` doesn't cross the C boundary directly — this maps the
/// small integer Rust sends to the real enum case.
private func version(from code: Int32) -> AsrModelVersion? {
    switch code {
    case 0: return .v3
    case 1: return .v2
    default: return nil
    }
}

/// Loads the given model version, downloading it if not already cached —
/// `AsrModels.downloadAndLoad` handles both the download-and-verify and the
/// load-from-disk cases uniformly, so there is no separate "is it cached"
/// branch on the Rust side for this backend.
///
/// Deliberately always uses FluidAudio's own default cache directory
/// (`to:` left `nil`), not a para-supplied path: `AsrModels.load(from:)`
/// derives its actual model-file directory from `directory
/// .deletingLastPathComponent()` plus the repo's own folder name — a
/// directory-handling quirk that made a custom `to:` behave inconsistently
/// with `download(to:)`'s own (different) interpretation of the same
/// parameter when tested directly. Matching FluidAudio's own default
/// avoids that inconsistency entirely; `--cache-dir` on the Rust side no
/// longer controls where these particular model files live (para's own
/// vendored data is unaffected — see main.rs).
/// `cpuOnly != 0` forces `MLComputeUnits.cpuOnly` (contracts/cli-interface.md
/// `--device cpu` — deterministic, no Neural Engine/GPU dispatch, useful for
/// benchmarking or troubleshooting); otherwise FluidAudio's own default
/// (`.cpuAndNeuralEngine`, real ANE acceleration) is used.
@_cdecl("para_load_model")
public func para_load_model(
    _ ptr: UnsafeMutableRawPointer?, _ versionCode: Int32, _ cpuOnly: Int32
) -> Int32 {
    guard let ptr else { return -1 }
    let state = Unmanaged<BridgeState>.fromOpaque(ptr).takeUnretainedValue()
    guard let v = version(from: versionCode) else {
        state.lastError = "unknown model version code \(versionCode)"
        return -1
    }

    let semaphore = DispatchSemaphore(value: 0)
    var loadError: Error?

    Task {
        do {
            let config: MLModelConfiguration? =
                cpuOnly != 0
                ? {
                    let c = AsrModels.defaultConfiguration()
                    c.computeUnits = .cpuOnly
                    return c
                }() : nil
            let models = try await AsrModels.downloadAndLoad(
                configuration: config, version: v,
                encoderComputeUnits: cpuOnly != 0 ? .cpuOnly : nil)
            let manager = AsrManager()
            try await manager.loadModels(models)
            state.manager = manager
        } catch {
            loadError = error
        }
        semaphore.signal()
    }
    semaphore.wait()

    if let loadError {
        state.lastError = String(describing: loadError)
        return -1
    }
    return 0
}

/// Whether `version`'s model files are already fully cached — used for
/// `--list-models`'s cache-state report. No network access, no loading.
@_cdecl("para_model_is_cached")
public func para_model_is_cached(_ versionCode: Int32) -> Int32 {
    guard let v = version(from: versionCode) else { return -1 }
    let dir = AsrModels.defaultCacheDirectory(for: v)
    return AsrModels.modelsExist(at: dir, version: v) ? 1 : 0
}

/// Deletes and re-downloads `version`'s cached files (`--refresh-model`).
/// Does not load the model afterward — a subsequent `para_load_model` call
/// does that, exactly like a normal cold run would.
@_cdecl("para_refresh_model")
public func para_refresh_model(_ ptr: UnsafeMutableRawPointer?, _ versionCode: Int32) -> Int32 {
    guard let ptr else { return -1 }
    let state = Unmanaged<BridgeState>.fromOpaque(ptr).takeUnretainedValue()
    guard let v = version(from: versionCode) else {
        state.lastError = "unknown model version code \(versionCode)"
        return -1
    }

    let semaphore = DispatchSemaphore(value: 0)
    var downloadError: Error?

    Task {
        do {
            _ = try await AsrModels.download(force: true, version: v)
        } catch {
            downloadError = error
        }
        semaphore.signal()
    }
    semaphore.wait()

    if let downloadError {
        state.lastError = String(describing: downloadError)
        return -1
    }
    return 0
}

/// One word's timing, mirrored into a flat parallel-array C ABI (matching
/// the diarization-segment pattern fluidaudio-rs already uses for
/// arrays-of-timed-things across this same kind of boundary).
@_cdecl("para_transcribe_file")
public func para_transcribe_file(
    _ ptr: UnsafeMutableRawPointer?,
    _ path: UnsafePointer<CChar>?,
    _ outText: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?,
    _ outWords: UnsafeMutablePointer<UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?>?,
    _ outWordStarts: UnsafeMutablePointer<UnsafeMutablePointer<Double>?>?,
    _ outWordEnds: UnsafeMutablePointer<UnsafeMutablePointer<Double>?>?,
    _ outWordCount: UnsafeMutablePointer<UInt32>?
) -> Int32 {
    guard let ptr, let path, let outText, let outWords, let outWordStarts, let outWordEnds,
        let outWordCount
    else { return -1 }
    let state = Unmanaged<BridgeState>.fromOpaque(ptr).takeUnretainedValue()

    guard let manager = state.manager else {
        state.lastError = BridgeError.notInitialized.description
        return -1
    }
    guard let pathStr = String(validatingCString: path) else {
        state.lastError = BridgeError.invalidUTF8Path.description
        return -1
    }

    let semaphore = DispatchSemaphore(value: 0)
    var result: ASRResult?
    var transcribeError: Error?

    Task {
        do {
            // Fresh decoder state per call: the TDT decoder's LSTM
            // hidden/cell state and last-token would otherwise persist
            // across calls, biasing the next transcription's predictor
            // (fluidaudio-rs's own bridge documents this same requirement).
            var decoderState = try TdtDecoderState()
            let url = URL(fileURLWithPath: pathStr)
            result = try await manager.transcribe(url, decoderState: &decoderState)
        } catch {
            transcribeError = error
        }
        semaphore.signal()
    }
    semaphore.wait()

    if let transcribeError {
        state.lastError = String(describing: transcribeError)
        return -1
    }
    guard let r = result else {
        state.lastError = BridgeError.noResult.description
        return -1
    }

    outText.pointee = cString(r.text)

    let words = buildWordTimings(from: r.tokenTimings ?? [])
    let count = words.count
    let wordPtrs = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>.allocate(capacity: count)
    let starts = UnsafeMutablePointer<Double>.allocate(capacity: count)
    let ends = UnsafeMutablePointer<Double>.allocate(capacity: count)
    for (i, w) in words.enumerated() {
        wordPtrs[i] = cString(w.word)
        starts[i] = w.startTime
        ends[i] = w.endTime
    }
    outWords.pointee = count > 0 ? wordPtrs : nil
    outWordStarts.pointee = count > 0 ? starts : nil
    outWordEnds.pointee = count > 0 ? ends : nil
    outWordCount.pointee = UInt32(count)

    return 0
}

@_cdecl("para_free_transcribe_result")
public func para_free_transcribe_result(
    _ text: UnsafeMutablePointer<CChar>?,
    _ words: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?,
    _ wordStarts: UnsafeMutablePointer<Double>?,
    _ wordEnds: UnsafeMutablePointer<Double>?,
    _ wordCount: UInt32
) {
    if let text { text.deallocate() }
    if let words {
        for i in 0..<Int(wordCount) {
            if let w = words[i] { w.deallocate() }
        }
        words.deallocate()
    }
    wordStarts?.deallocate()
    wordEnds?.deallocate()
}

@_cdecl("para_bridge_last_error")
public func para_bridge_last_error(
    _ ptr: UnsafeMutableRawPointer?
) -> UnsafeMutablePointer<CChar>? {
    withState(ptr) { cString($0.lastError) } ?? nil
}

/// Frees a single C string previously returned by `para_bridge_last_error`.
/// Deallocates via Swift's own allocator (`UnsafeMutablePointer.deallocate`),
/// matching how `cString(_:)` allocated it — not `libc::free`, which is not
/// guaranteed-safe for memory Swift's allocator produced.
@_cdecl("para_free_error_string")
public func para_free_error_string(_ s: UnsafeMutablePointer<CChar>?) {
    s?.deallocate()
}
