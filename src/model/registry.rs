use crate::inference::ModelKind;

/// Which timing resolution a model's decode path produces (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingGranularity {
    /// Phrase/sentence-level segments (TDT models).
    Segment,
    /// One segment spanning the whole input (CTC models — research.md §3).
    WholeFile,
}

/// Which pipeline stage a `ModelFile` feeds (data-model.md). The mel
/// preprocessor is deliberately absent — it's vendored, not downloaded
/// (research.md §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    Encoder,
    DecoderJoint,
    Vocab,
}

#[derive(Debug, Clone)]
pub struct ModelFile {
    /// Filename on disk within the model's cache directory.
    pub name: &'static str,
    pub role: FileRole,
    /// HuggingFace `resolve/main/<filename>` URL.
    pub source_url: &'static str,
    /// Lowercase hex SHA-256, computed from the real downloaded file.
    ///
    /// `None` here means "not yet verified against a real download" — this
    /// must never be treated as a valid checksum to skip against.
    /// Constitution Principle V forbids inventing a value; `manager.rs`'s
    /// checksum check must treat `None` as "verification required before
    /// this entry can be used," not as "skip verification."
    pub sha256: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: &'static str,
    pub description: &'static str,
    pub kind: ModelKind,
    pub timing_granularity: TimingGranularity,
    pub is_default: bool,
    pub files: &'static [ModelFile],
}

pub const MODELS: &[ModelEntry] = &[
    ModelEntry {
        id: "parakeet-tdt-0.6b-v3",
        description: "Multilingual (25 European languages), automatic language detection, word-level timestamps. Default model.",
        kind: ModelKind::Tdt,
        timing_granularity: TimingGranularity::Segment,
        is_default: true,
        files: &[
            ModelFile {
                name: "encoder-model.onnx",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.onnx",
                sha256: Some("98a74b21b4cc0017c1e7030319a4a96f4a9506e50f0708f3a516d02a77c96bb1"),
            },
            ModelFile {
                name: "encoder-model.onnx.data",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.onnx.data",
                sha256: Some("9a22d372c51455c34f13405da2520baefb7125bd16981397561423ed32d24f36"),
            },
            ModelFile {
                name: "decoder_joint-model.onnx",
                role: FileRole::DecoderJoint,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/decoder_joint-model.onnx",
                sha256: Some("e978ddf6688527182c10fde2eb4b83068421648985ef23f7a86be732be8706c1"),
            },
            ModelFile {
                name: "vocab.txt",
                role: FileRole::Vocab,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt",
                sha256: Some("d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d"),
            },
        ],
    },
    ModelEntry {
        id: "parakeet-tdt-0.6b-v2",
        description: "English only, word-level timestamps. Retained for compatibility.",
        kind: ModelKind::Tdt,
        timing_granularity: TimingGranularity::Segment,
        is_default: false,
        files: &[
            ModelFile {
                name: "encoder-model.onnx",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/encoder-model.onnx",
                sha256: Some("3987bcd28175d829d12888a996a84e8f62a0e374d9ffd640662c1515adc679d3"),
            },
            ModelFile {
                name: "encoder-model.onnx.data",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/encoder-model.onnx.data",
                sha256: Some("4dab7362d4874d85965045b1e41b2d61dd2cc0fb25671a7f6b3dc47bf120cc41"),
            },
            ModelFile {
                name: "decoder_joint-model.onnx",
                role: FileRole::DecoderJoint,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/decoder_joint-model.onnx",
                sha256: Some("cbb52a07bd70ab5b67f8439d4b3cd8704b18467b4430bcacb5adabe154b8d191"),
            },
            ModelFile {
                name: "vocab.txt",
                role: FileRole::Vocab,
                source_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/vocab.txt",
                sha256: Some("ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d"),
            },
        ],
    },
    ModelEntry {
        id: "parakeet-ctc-0.6b",
        description: "English only. No per-word timestamps (whole-file single segment). Fastest tier — single forward pass, no autoregressive decode.",
        kind: ModelKind::Ctc,
        timing_granularity: TimingGranularity::WholeFile,
        is_default: false,
        files: &[
            ModelFile {
                name: "model.onnx",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-ctc-0.6b-onnx/resolve/main/model.onnx",
                sha256: Some("4d2866ba5d0162d870776bad53991f29dac04238d1d7b1e02874b43ebc5c702a"),
            },
            ModelFile {
                name: "model.onnx.data",
                role: FileRole::Encoder,
                source_url: "https://huggingface.co/istupakov/parakeet-ctc-0.6b-onnx/resolve/main/model.onnx.data",
                sha256: Some("e1951d4e0bdcde6e3059b8796140d98d73b3b9c7bee1adbc60505ceb09b9473f"),
            },
            ModelFile {
                name: "vocab.txt",
                role: FileRole::Vocab,
                source_url: "https://huggingface.co/istupakov/parakeet-ctc-0.6b-onnx/resolve/main/vocab.txt",
                sha256: Some("ed16e1a4e3a3aa379138c0b1888e5d49f993c9d512b2be4d46e90a87afd54921"),
            },
        ],
    },
];

pub fn default_model() -> &'static ModelEntry {
    MODELS
        .iter()
        .find(|m| m.is_default)
        .expect("registry must have exactly one default model")
}

pub fn find(id: &str) -> Option<&'static ModelEntry> {
    MODELS.iter().find(|m| m.id == id)
}

/// Implements `--list-models` (FR-019): every registered model, its cache
/// state, and which one is the default. Exits the caller's `run()` with
/// success — this function only prints; it does not transcribe.
pub fn list_models() -> anyhow::Result<()> {
    println!("Available models:\n");
    for entry in MODELS {
        let default_marker = if entry.is_default { "  (default)" } else { "" };
        println!("  {}{}", entry.id, default_marker);
        println!("    {}", entry.description);
        let state = crate::model::manager::cache_state(entry)?;
        println!("    Cache state: {state:?}\n");
    }
    println!("Use --model <id> to select a model.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_required_fields() {
        for entry in MODELS {
            assert!(!entry.id.is_empty());
            assert!(!entry.files.is_empty());
            for file in entry.files {
                assert!(!file.name.is_empty());
                assert!(file.source_url.starts_with("https://huggingface.co"));
            }
        }
    }

    #[test]
    fn exactly_one_default_model() {
        assert_eq!(MODELS.iter().filter(|m| m.is_default).count(), 1);
    }

    #[test]
    fn default_model_resolves() {
        assert_eq!(default_model().id, "parakeet-tdt-0.6b-v3");
    }

    #[test]
    fn at_least_three_models_per_fr_008() {
        assert!(MODELS.len() >= 3);
    }

    #[test]
    fn find_unknown_model_returns_none() {
        assert!(find("does-not-exist").is_none());
    }
}
