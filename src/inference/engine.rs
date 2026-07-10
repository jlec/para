use crate::inference::Device;
use anyhow::Context;
use ort::ep::{CPU, CoreML, ExecutionProviderDispatch};
use ort::session::Session;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static COREML_NOTICE_SHOWN: AtomicBool = AtomicBool::new(false);

/// Whether this target could plausibly run the CoreML execution provider.
/// (`Device::Auto` only tries CoreML on darwin/aarch64 — Constitution Principle VI.)
fn coreml_capable_target() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

fn execution_providers(device: Device) -> Vec<ExecutionProviderDispatch> {
    match device {
        Device::Cpu => vec![CPU::default().build()],
        Device::Coreml => vec![CoreML::default().build()],
        Device::Auto if coreml_capable_target() => {
            vec![CoreML::default().build(), CPU::default().build()]
        }
        Device::Auto => vec![CPU::default().build()],
    }
}

/// Emits the CoreML first-compile stderr notice at most once per process,
/// before the session is built, so a user doesn't mistake a silent 30-60s
/// compile for a hang. Printed whenever CoreML will be attempted at all; it
/// does not try to detect whether ONNX Runtime's own compilation cache
/// already has a hit for this exact model, so it may print once even on a
/// warm-cache repeat run — a deliberate simplification over tracking that
/// cache's state ourselves.
fn maybe_emit_coreml_notice(device: Device) {
    let will_try_coreml = matches!(device, Device::Coreml)
        || matches!(device, Device::Auto if coreml_capable_target());
    if will_try_coreml
        && COREML_NOTICE_SHOWN
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        eprintln!(
            "Note: compiling model for Apple Neural Engine (first run only — may take up to a minute)"
        );
    }
}

/// Builds an ONNX Runtime session from a file on disk (used for the
/// downloaded encoder/decoder-joint graphs).
pub fn build_session_from_file(path: &Path, device: Device) -> anyhow::Result<Session> {
    maybe_emit_coreml_notice(device);
    Session::builder()
        .context("failed to create ONNX Runtime session builder")?
        .with_execution_providers(execution_providers(device))
        .map_err(|e| anyhow::anyhow!("failed to configure execution providers: {e}"))?
        .commit_from_file(path)
        .with_context(|| format!("failed to load ONNX model: {}", path.display()))
}

/// Builds an ONNX Runtime session from an in-memory byte slice (used for the
/// vendored preprocessor graphs — research.md §10; no file on disk at all).
pub fn build_session_from_memory(bytes: &[u8], device: Device) -> anyhow::Result<Session> {
    maybe_emit_coreml_notice(device);
    Session::builder()
        .context("failed to create ONNX Runtime session builder")?
        .with_execution_providers(execution_providers(device))
        .map_err(|e| anyhow::anyhow!("failed to configure execution providers: {e}"))?
        .commit_from_memory(bytes)
        .context("failed to load vendored preprocessor model from memory")
}
