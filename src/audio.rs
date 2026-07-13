use anyhow::{Context, anyhow};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locates `ffmpeg` on `PATH` (FR-001; Constitution Principle VII — the only
/// dependency the user installs manually).
pub fn find_ffmpeg() -> anyhow::Result<PathBuf> {
    which::which("ffmpeg")
        .map_err(|_| anyhow!("ffmpeg not found on PATH — install with: brew install ffmpeg"))
}

/// Locates `ffprobe`, which ships alongside `ffmpeg` in the same install
/// (e.g. the Homebrew `ffmpeg` formula) and is used only for probing.
fn find_ffprobe() -> anyhow::Result<PathBuf> {
    which::which("ffprobe")
        .map_err(|_| anyhow!("ffprobe not found on PATH — install with: brew install ffmpeg"))
}

/// What we know about an input file before transcoding it (data-model.md `InputMedia`).
#[derive(Debug, Clone, Copy)]
pub struct ProbeInfo {
    pub duration_secs: f64,
    pub has_audio_track: bool,
}

/// Probes `input` for duration and audio-track presence via `ffprobe`. Used
/// to reject unusable input (FR-015) before spending time transcoding it.
pub fn probe(input: &Path) -> anyhow::Result<ProbeInfo> {
    let ffprobe = find_ffprobe()?;
    let output = Command::new(ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration:stream=codec_type")
        .arg("-of")
        .arg("json")
        .arg(input)
        .output()
        .with_context(|| format!("failed to run ffprobe on {}", input.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "input file not readable: {}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("could not parse ffprobe output for {}", input.display()))?;

    let duration_secs = parsed["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let has_audio_track = parsed["streams"]
        .as_array()
        .map(|streams| streams.iter().any(|s| s["codec_type"] == "audio"))
        .unwrap_or(false);

    Ok(ProbeInfo {
        duration_secs,
        has_audio_track,
    })
}

/// Transcodes `input` to 16 kHz mono 16-bit PCM WAV at `dest` (the format
/// Parakeet's ONNX models require). Extracts audio from video automatically
/// (FR-001, FR-003) — no separate manual step.
///
/// Paths are passed to `Command::arg` directly via `AsRef<OsStr>` rather than
/// through `.to_str().unwrap()`, so a non-UTF-8 path cannot panic here
/// (Constitution Engineering Standards: no panics in library code).
pub fn transcode_to_wav(input: &Path, dest: &Path) -> anyhow::Result<()> {
    let ffmpeg = find_ffmpeg()?;
    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-ar")
        .arg("16000")
        .arg("-ac")
        .arg("1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg("-f")
        .arg("wav")
        .arg(dest)
        .output()
        .with_context(|| format!("failed to run ffmpeg on {}", input.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "audio transcoding failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Stages stdin's raw bytes into a temp file so `ffmpeg` (which cannot seek
/// arbitrary stdin streams for all formats) can operate on a real path
/// (FR-002). The temp file is deleted automatically when dropped.
pub fn stage_stdin() -> anyhow::Result<tempfile::NamedTempFile> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .context("failed to read audio data from stdin")?;
    let mut file =
        tempfile::NamedTempFile::new().context("failed to create temp file for stdin input")?;
    std::io::Write::write_all(&mut file, &buf).context("failed to stage stdin input to disk")?;
    Ok(file)
}

/// Reads a mono 16-bit PCM WAV file (the exact format `transcode_to_wav`
/// produces) into normalized `f32` samples — `int16 / 32768.0`, the same
/// scaling the reference `onnx-asr` implementation's `read_wav` uses.
/// Locates the `data` chunk properly rather than assuming a fixed header
/// offset, since `fmt ` chunk size can vary.
pub fn read_wav_samples(path: &Path) -> anyhow::Result<Vec<f32>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read wav file {}", path.display()))?;
    anyhow::ensure!(
        bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "{} is not a valid WAV file",
        path.display()
    );

    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let data_start = pos + 8;
        if chunk_id == b"data" {
            let data_end = (data_start + chunk_size).min(bytes.len());
            let samples = bytes[data_start..data_end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect();
            return Ok(samples);
        }
        // Chunks are word-aligned; an odd-sized chunk has a padding byte.
        pos = data_start + chunk_size + (chunk_size % 2);
    }
    anyhow::bail!("{}: no data chunk found in WAV file", path.display())
}

/// Magic-byte format detection for diagnostics only (FR-001's Assumption:
/// unrecognized formats are still passed to ffmpeg without error).
pub fn detect_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 {
        return None;
    }
    if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("wav")
    } else if &bytes[0..3] == b"ID3" || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
        Some("mp3")
    } else if &bytes[4..8] == b"ftyp" {
        Some("m4a/mp4")
    } else if bytes[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        Some("mkv/webm")
    } else if &bytes[0..4] == b"fLaC" {
        Some("flac")
    } else if &bytes[0..4] == b"OggS" {
        Some("ogg/opus")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wav_magic_bytes() {
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"WAVE");
        assert_eq!(detect_format(&bytes), Some("wav"));
    }

    #[test]
    fn detects_flac_magic_bytes() {
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(b"fLaC");
        assert_eq!(detect_format(&bytes), Some("flac"));
    }

    #[test]
    fn detects_ogg_magic_bytes() {
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(b"OggS");
        assert_eq!(detect_format(&bytes), Some("ogg/opus"));
    }

    #[test]
    fn unknown_bytes_return_none() {
        let bytes = vec![0u8; 12];
        assert_eq!(detect_format(&bytes), None);
    }

    #[test]
    fn short_input_returns_none() {
        assert_eq!(detect_format(&[0u8; 4]), None);
    }
}
