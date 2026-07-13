//! Shared helpers for contract/integration tests.

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn para_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_para"))
}

/// Writes a minimal valid mono 16-bit PCM WAV file of `seconds` of silence at
/// `path` — enough for ffmpeg/the mel preprocessor to accept as real input
/// without depending on any real speech recording.
pub fn write_silence_wav(path: &Path, seconds: f64) {
    let sample_rate: u32 = 16_000;
    let num_samples = (sample_rate as f64 * seconds) as u32;
    let data_size = num_samples * 2; // 16-bit mono
    let mut file = std::fs::File::create(path).unwrap();

    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // fmt chunk size
    file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    file.write_all(&1u16.to_le_bytes()).unwrap(); // mono
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&(sample_rate * 2).to_le_bytes()).unwrap(); // byte rate
    file.write_all(&2u16.to_le_bytes()).unwrap(); // block align
    file.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample

    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();
    file.write_all(&vec![0u8; data_size as usize]).unwrap();
}
