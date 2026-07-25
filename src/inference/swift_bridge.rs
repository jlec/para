//! Safe Rust wrapper around `ParaBridge` (`swift/Sources/ParaBridge`), the
//! Swift shim that links FluidAudio's real Swift ASR library directly
//! (specs/004-native-coreml-backend/research.md) — no ONNX Runtime, no
//! ONNX-format models, no reimplementation of FluidAudio's chunking/decoding
//! logic in Rust. `build.rs` compiles `swift/` and links `libParaBridge.a`
//! plus the Apple frameworks it needs into this binary.

use anyhow::{Context, anyhow};
use std::ffi::{CStr, CString, c_char};
use std::path::Path;

/// Which real FluidAudio model version to load — matches `version(from:)`'s
/// integer mapping in `swift/Sources/ParaBridge/ParaBridge.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelVersion {
    V3,
    V2,
}

impl ModelVersion {
    fn code(self) -> i32 {
        match self {
            ModelVersion::V3 => 0,
            ModelVersion::V2 => 1,
        }
    }
}

unsafe extern "C" {
    fn para_bridge_create() -> *mut std::ffi::c_void;
    fn para_bridge_destroy(bridge: *mut std::ffi::c_void);
    fn para_bridge_last_error(bridge: *mut std::ffi::c_void) -> *mut c_char;
    fn para_free_error_string(s: *mut c_char);

    fn para_load_model(bridge: *mut std::ffi::c_void, version_code: i32, cpu_only: i32) -> i32;
    fn para_model_is_cached(version_code: i32) -> i32;
    fn para_refresh_model(bridge: *mut std::ffi::c_void, version_code: i32) -> i32;

    fn para_transcribe_file(
        bridge: *mut std::ffi::c_void,
        path: *const c_char,
        out_text: *mut *mut c_char,
        out_words: *mut *mut *mut c_char,
        out_word_starts: *mut *mut f64,
        out_word_ends: *mut *mut f64,
        out_word_count: *mut u32,
    ) -> i32;

    fn para_free_transcribe_result(
        text: *mut c_char,
        words: *mut *mut c_char,
        word_starts: *mut f64,
        word_ends: *mut f64,
        word_count: u32,
    );
}

/// One word's timing, as produced by FluidAudio's own `buildWordTimings`
/// (real word-boundary-aware grouping of sub-word tokens, not something
/// this crate computes itself).
#[derive(Debug, Clone)]
pub struct WordTiming {
    pub word: String,
    pub start_secs: f64,
    pub end_secs: f64,
}

/// The result of one file transcribed via the native CoreML backend. Only
/// `words` is kept — para reconstructs its own display text from these
/// (filler-word removal, paragraph breaks; `inference::segments`) rather
/// than using FluidAudio's raw `ASRResult.text` directly.
#[derive(Debug, Clone)]
pub struct SwiftTranscript {
    pub words: Vec<WordTiming>,
}

/// Checks whether `version`'s model files are already fully cached, with no
/// network access and without creating a bridge instance — backs
/// `--list-models`'s cache-state report.
pub fn model_is_cached(version: ModelVersion) -> anyhow::Result<bool> {
    match unsafe { para_model_is_cached(version.code()) } {
        1 => Ok(true),
        0 => Ok(false),
        _ => anyhow::bail!("failed to check model cache state"),
    }
}

/// Owns one loaded FluidAudio model plus decoder state, via an opaque
/// pointer into Swift-managed memory. `Drop` releases it.
pub struct SwiftAsrBridge {
    ptr: *mut std::ffi::c_void,
}

// The Swift side funnels every call through a semaphore-blocked `Task`, so
// at most one FluidAudio call is in flight per bridge instance at a time —
// safe to move between threads as long as calls aren't made concurrently,
// which nothing in this codebase does (one bridge per `para` invocation).
unsafe impl Send for SwiftAsrBridge {}

impl SwiftAsrBridge {
    /// Creates a new bridge instance. Fails only if the Swift side's
    /// allocation itself fails, which real-world testing has never observed —
    /// still surfaced as a real error rather than assumed infallible.
    pub fn new() -> anyhow::Result<Self> {
        let ptr = unsafe { para_bridge_create() };
        if ptr.is_null() {
            anyhow::bail!("failed to create ParaBridge instance");
        }
        Ok(Self { ptr })
    }

    fn last_error(&self) -> String {
        unsafe {
            let raw = para_bridge_last_error(self.ptr);
            if raw.is_null() {
                return "unknown error (no message from Swift bridge)".to_string();
            }
            let msg = CStr::from_ptr(raw).to_string_lossy().into_owned();
            para_free_error_string(raw);
            msg
        }
    }

    /// Loads `version`, downloading it via FluidAudio's own
    /// `AsrModels.downloadAndLoad` (into FluidAudio's own default cache
    /// directory — see `ParaBridge.swift`'s `para_load_model` doc comment
    /// for why a para-supplied directory isn't used) if not already cached.
    /// `cpu_only` forces `MLComputeUnits.cpuOnly` (`--device cpu`); otherwise
    /// FluidAudio's own default (real Neural Engine acceleration) is used.
    pub fn load_model(&mut self, version: ModelVersion, cpu_only: bool) -> anyhow::Result<()> {
        let rc = unsafe { para_load_model(self.ptr, version.code(), cpu_only as i32) };
        if rc != 0 {
            anyhow::bail!("failed to load native CoreML model: {}", self.last_error());
        }
        Ok(())
    }

    /// Deletes and re-downloads `version`'s cached files (`--refresh-model`).
    pub fn refresh_model(&mut self, version: ModelVersion) -> anyhow::Result<()> {
        let rc = unsafe { para_refresh_model(self.ptr, version.code()) };
        if rc != 0 {
            anyhow::bail!("failed to refresh model: {}", self.last_error());
        }
        Ok(())
    }

    /// Transcribes one audio file via the loaded model, returning full text
    /// plus real, FluidAudio-produced word-level timings.
    pub fn transcribe_file(&mut self, path: &Path) -> anyhow::Result<SwiftTranscript> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow!("input file path is not valid UTF-8"))?;
        let c_path = CString::new(path_str).context("input file path contains a NUL byte")?;

        let mut out_text: *mut c_char = std::ptr::null_mut();
        let mut out_words: *mut *mut c_char = std::ptr::null_mut();
        let mut out_word_starts: *mut f64 = std::ptr::null_mut();
        let mut out_word_ends: *mut f64 = std::ptr::null_mut();
        let mut out_word_count: u32 = 0;

        let rc = unsafe {
            para_transcribe_file(
                self.ptr,
                c_path.as_ptr(),
                &mut out_text,
                &mut out_words,
                &mut out_word_starts,
                &mut out_word_ends,
                &mut out_word_count,
            )
        };
        if rc != 0 {
            anyhow::bail!("native CoreML transcription failed: {}", self.last_error());
        }

        let count = out_word_count as usize;
        let mut words = Vec::with_capacity(count);
        for i in 0..count {
            unsafe {
                let word_ptr = *out_words.add(i);
                let word = CStr::from_ptr(word_ptr).to_string_lossy().into_owned();
                let start_secs = *out_word_starts.add(i);
                let end_secs = *out_word_ends.add(i);
                words.push(WordTiming {
                    word,
                    start_secs,
                    end_secs,
                });
            }
        }

        unsafe {
            para_free_transcribe_result(
                out_text,
                out_words,
                out_word_starts,
                out_word_ends,
                out_word_count,
            );
        }

        Ok(SwiftTranscript { words })
    }
}

impl Drop for SwiftAsrBridge {
    fn drop(&mut self) {
        unsafe { para_bridge_destroy(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "downloads real multi-GB FluidAudio CoreML models on first run"]
    fn transcribes_real_file_via_native_coreml() {
        let mut bridge = SwiftAsrBridge::new().unwrap();
        bridge.load_model(ModelVersion::V3, false).unwrap();
        let result = bridge
            .transcribe_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("test.wav")
                    .as_path(),
            )
            .unwrap();
        assert!(!result.words.is_empty());
        assert!(!result.words[0].word.is_empty());
    }
}
