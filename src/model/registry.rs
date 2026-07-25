use crate::inference::swift_bridge::ModelVersion;

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: &'static str,
    pub description: &'static str,
    pub version: ModelVersion,
    pub is_default: bool,
}

/// The two real, published FluidAudio CoreML Parakeet TDT conversions
/// (specs/004-native-coreml-backend/research.md §1) — `parakeet-ctc-0.6b`
/// was dropped along with the ONNX Runtime pipeline it depended on:
/// FluidAudio has no equivalent standalone CTC transcription API this
/// project found a clean path to (see research.md's amendment note).
pub const MODELS: &[ModelEntry] = &[
    ModelEntry {
        id: "parakeet-tdt-0.6b-v3",
        description: "Multilingual (25 European languages), automatic language detection, word-level timestamps. Default model.",
        version: ModelVersion::V3,
        is_default: true,
    },
    ModelEntry {
        id: "parakeet-tdt-0.6b-v2",
        description: "English only, word-level timestamps. Retained for compatibility.",
        version: ModelVersion::V2,
        is_default: false,
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
        let cached = crate::inference::swift_bridge::model_is_cached(entry.version)?;
        let state = if cached { "Cached" } else { "NotCached" };
        println!("    Cache state: {state}\n");
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
            assert!(!entry.description.is_empty());
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
    fn find_unknown_model_returns_none() {
        assert!(find("does-not-exist").is_none());
    }
}
