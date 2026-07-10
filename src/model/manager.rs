use crate::model::registry::{ModelEntry, ModelFile};
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Bounded retry count for a single file's download (FR-022; research.md §7:
/// 3 attempts, exponential backoff, then loud failure — never a silent
/// fallback to a different cached model).
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    NotCached,
    Cached,
}

#[derive(Debug, thiserror::Error)]
enum DownloadError {
    #[error("network error downloading {file}: {source}")]
    Network {
        file: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("io error for {file}: {source}")]
    Io {
        file: String,
        #[source]
        source: std::io::Error,
    },
}

impl DownloadError {
    /// Whether retrying the same download might help. A checksum mismatch on
    /// a fully-downloaded file is not fixed by blindly retrying — the bytes
    /// already didn't match, so this is treated as terminal for this attempt
    /// loop and reported clearly rather than retried into a busy-loop.
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            DownloadError::Network { .. } | DownloadError::Io { .. }
        )
    }
}

/// Resolves the root cache directory: `--cache-dir`/`PARA_CACHE_DIR` if
/// given (already resolved by clap's `env` attribute before reaching here),
/// otherwise the OS cache directory.
pub fn cache_root(override_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    match override_dir {
        Some(p) => Ok(p.to_path_buf()),
        None => Ok(dirs::cache_dir()
            .context("could not determine the OS cache directory")?
            .join("para")),
    }
}

fn model_dir(entry: &ModelEntry, override_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    Ok(cache_root(override_dir)?.join("models").join(entry.id))
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("failed to hash {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Checks whether every file for `entry` is present (and, where a checksum
/// is known, matches it) in the default cache location.
pub fn cache_state(entry: &ModelEntry) -> anyhow::Result<CacheState> {
    cache_state_in(entry, None)
}

pub fn cache_state_in(
    entry: &ModelEntry,
    override_dir: Option<&Path>,
) -> anyhow::Result<CacheState> {
    let dir = model_dir(entry, override_dir)?;
    for file in entry.files {
        let path = dir.join(file.name);
        if !path.exists() {
            return Ok(CacheState::NotCached);
        }
        if let Some(expected) = file.sha256 {
            if sha256_file(&path)? != expected {
                return Ok(CacheState::NotCached);
            }
        }
    }
    Ok(CacheState::Cached)
}

/// Guards a model directory's download with a `download.lock` file so two
/// concurrent `para` invocations don't race on the same cache entry. Cleans
/// up stale `.tmp` files from any previously-interrupted download on
/// acquire. Removed on drop (clean exit) — see the README's note on signals:
/// this doesn't run on SIGKILL, same caveat as the stdin temp file.
struct DownloadLock {
    path: PathBuf,
}

impl DownloadLock {
    fn acquire(dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                let _ = fs::remove_file(&path);
            }
        }

        let lock_path = dir.join("download.lock");
        let mut waited = Duration::ZERO;
        while lock_path.exists() && waited < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(500));
            waited += Duration::from_millis(500);
        }
        fs::write(&lock_path, b"")?;
        Ok(Self { path: lock_path })
    }
}

impl Drop for DownloadLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn download_one(file: &ModelFile, dest_dir: &Path) -> Result<(), DownloadError> {
    let dest = dest_dir.join(file.name);
    let tmp = dest_dir.join(format!("{}.tmp", file.name));

    let mut response =
        reqwest::blocking::get(file.source_url).map_err(|source| DownloadError::Network {
            file: file.name.to_string(),
            source,
        })?;
    let total = response.content_length().unwrap_or(0);
    let bar = indicatif::ProgressBar::new(total);
    bar.set_style(
        indicatif::ProgressStyle::with_template("{msg} [{bar:40}] {bytes}/{total_bytes}")
            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
    );
    bar.set_message(file.name);

    let mut out = fs::File::create(&tmp).map_err(|source| DownloadError::Io {
        file: file.name.to_string(),
        source,
    })?;
    let mut buf = [0u8; 65536];
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|source| DownloadError::Io {
                file: file.name.to_string(),
                source,
            })?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|source| DownloadError::Io {
                file: file.name.to_string(),
                source,
            })?;
        bar.inc(n as u64);
    }
    bar.finish_and_clear();
    drop(out);

    if let Some(expected) = file.sha256 {
        let actual = sha256_file(&tmp).map_err(|_| DownloadError::Io {
            file: file.name.to_string(),
            source: std::io::Error::other("failed to hash downloaded file"),
        })?;
        if actual != expected {
            let _ = fs::remove_file(&tmp);
            return Err(DownloadError::ChecksumMismatch {
                file: file.name.to_string(),
                expected: expected.to_string(),
                actual,
            });
        }
    }

    fs::rename(&tmp, &dest).map_err(|source| DownloadError::Io {
        file: file.name.to_string(),
        source,
    })?;
    Ok(())
}

fn download_one_with_retry(file: &ModelFile, dest_dir: &Path) -> anyhow::Result<()> {
    let mut last_err = None;
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        match download_one(file, dest_dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.is_retryable() && attempt < MAX_DOWNLOAD_ATTEMPTS => {
                eprintln!(
                    "download of {} failed (attempt {attempt}/{MAX_DOWNLOAD_ATTEMPTS}): {e}; retrying...",
                    file.name
                );
                std::thread::sleep(Duration::from_secs(1 << (attempt - 1)));
                last_err = Some(e);
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(last_err
        .expect("loop always sets last_err before exhausting attempts")
        .into())
}

/// Ensures every file for `entry` is present and verified in the cache,
/// downloading whatever is missing. Never leaves a partially-downloaded file
/// in place (atomic rename on success only) and never falls back to a
/// different model on failure (FR-022).
pub fn ensure_cached(entry: &ModelEntry, override_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    let dir = model_dir(entry, override_dir)?;
    if cache_state_in(entry, override_dir)? == CacheState::Cached {
        return Ok(dir);
    }

    let _lock = DownloadLock::acquire(&dir)?;
    if cache_state_in(entry, override_dir)? == CacheState::Cached {
        return Ok(dir);
    }

    for file in entry.files {
        download_one_with_retry(file, &dir)
            .with_context(|| format!("failed to download {} for model {}", file.name, entry.id))?;
    }
    Ok(dir)
}

/// Forces a fresh re-download of `entry`'s cached files, discarding whatever
/// is currently cached first (FR-020; `--refresh-model`).
pub fn refresh(entry: &ModelEntry, override_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    let dir = model_dir(entry, override_dir)?;
    for file in entry.files {
        let path = dir.join(file.name);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove cached {}", path.display()))?;
        }
    }
    ensure_cached(entry, override_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::ModelKind;
    use crate::model::registry::{FileRole, TimingGranularity};

    fn fixture_entry() -> ModelEntry {
        ModelEntry {
            id: "test-model",
            description: "fixture",
            kind: ModelKind::Ctc,
            timing_granularity: TimingGranularity::WholeFile,
            is_default: false,
            files: &[ModelFile {
                name: "vocab.txt",
                role: FileRole::Vocab,
                source_url: "https://example.invalid/vocab.txt",
                sha256: None,
            }],
        }
    }

    #[test]
    fn not_cached_when_directory_does_not_exist() {
        let entry = fixture_entry();
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist-yet");
        assert_eq!(
            cache_state_in(&entry, Some(&missing)).unwrap(),
            CacheState::NotCached
        );
    }

    #[test]
    fn cached_when_all_files_present_and_no_checksum_recorded() {
        let entry = fixture_entry();
        let dir = tempfile::tempdir().unwrap();
        let model_path = dir.path().join("models").join(entry.id);
        fs::create_dir_all(&model_path).unwrap();
        fs::write(model_path.join("vocab.txt"), b"hello").unwrap();
        assert_eq!(
            cache_state_in(&entry, Some(dir.path())).unwrap(),
            CacheState::Cached
        );
    }
}
