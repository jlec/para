//! Fetches the two shared mel-spectrogram preprocessor ONNX graphs
//! (`nemo128.onnx` for TDT models, `nemo80.onnx` for CTC) at build time and
//! writes them into `OUT_DIR`, where `src/inference/mel.rs` embeds them via
//! `include_bytes!`. This keeps them out of git (no binary blobs committed —
//! the repo's `forbid-binary` policy) while still shipping with zero runtime
//! network calls: Constitution Principle VII's "fetched automatically at
//! build time" clause, the same pattern the `ort` crate already uses for the
//! ONNX Runtime library itself (research.md §10).
//!
//! Source: the versioned `onnx-asr` PyPI wheel — the one place both files
//! are reliably available (individual HuggingFace model repos bundle
//! `nemo128.onnx` inconsistently and never bundle `nemo80.onnx` at all).
//! Every checksum below was computed from a real download, not invented
//! (Constitution Principle V).

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

const WHEEL_URL: &str = "https://files.pythonhosted.org/packages/82/04/bdffd682cc38b43144b6528186c80451f219a05e3fd0eb331a548f455b9a/onnx_asr-0.11.0-py3-none-any.whl";
const WHEEL_SHA256: &str = "142d8b3ce7716684992826a269304f5ce9cf1c0fe704b751358e223f45d2a5cf";

struct Preprocessor {
    /// Path within the wheel zip.
    wheel_path: &'static str,
    /// Filename written to `OUT_DIR`.
    out_name: &'static str,
    sha256: &'static str,
}

const PREPROCESSORS: &[Preprocessor] = &[
    Preprocessor {
        wheel_path: "onnx_asr/preprocessors/data/nemo128.onnx",
        out_name: "nemo128.onnx",
        sha256: "95afc3b529db4f84e038461d7d02e090c5aa2d28c68bc6c83f4255a9b3562f60",
    },
    Preprocessor {
        wheel_path: "onnx_asr/preprocessors/data/nemo80.onnx",
        out_name: "nemo80.onnx",
        sha256: "ea9d24c9bc3ea5ff1b8a2796ad7d1168487b0d004ed1bd860d6d257ea71ac1b8",
    },
];

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir =
        std::env::var("OUT_DIR").expect("OUT_DIR is always set by cargo for build scripts");
    let out_dir = Path::new(&out_dir);

    // If every target file already exists with the right checksum (e.g. an
    // incremental build reusing the same OUT_DIR), skip the network entirely.
    let all_present = PREPROCESSORS.iter().all(|p| {
        let path = out_dir.join(p.out_name);
        std::fs::read(&path)
            .map(|bytes| sha256_hex(&bytes) == p.sha256)
            .unwrap_or(false)
    });
    if all_present {
        return;
    }

    let response = match reqwest::blocking::get(WHEEL_URL).and_then(|r| r.error_for_status()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "error: failed to download preprocessor assets from {WHEEL_URL}: {e}\n\
                 This build step needs network access on first build (Constitution Principle VII \
                 — fetched automatically at build time)."
            );
            std::process::exit(1);
        }
    };
    let mut wheel_bytes = Vec::new();
    if let Err(e) = response
        .take(200 * 1024 * 1024)
        .read_to_end(&mut wheel_bytes)
    {
        eprintln!("error: failed to read preprocessor wheel body: {e}");
        std::process::exit(1);
    }

    let actual_wheel_hash = sha256_hex(&wheel_bytes);
    if actual_wheel_hash != WHEEL_SHA256 {
        eprintln!(
            "error: preprocessor wheel checksum mismatch: expected {WHEEL_SHA256}, got {actual_wheel_hash}. \
             Refusing to extract from an unverified archive."
        );
        std::process::exit(1);
    }

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(wheel_bytes)).unwrap_or_else(|e| {
        eprintln!("error: preprocessor wheel is not a valid zip archive: {e}");
        std::process::exit(1);
    });

    for p in PREPROCESSORS {
        let mut entry = archive.by_name(p.wheel_path).unwrap_or_else(|e| {
            eprintln!(
                "error: {} not found in preprocessor wheel: {e}",
                p.wheel_path
            );
            std::process::exit(1);
        });
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap_or_else(|e| {
            eprintln!("error: failed to read {} from wheel: {e}", p.wheel_path);
            std::process::exit(1);
        });

        let actual = sha256_hex(&bytes);
        if actual != p.sha256 {
            eprintln!(
                "error: {} checksum mismatch: expected {}, got {actual}. Refusing to embed an unverified file.",
                p.wheel_path, p.sha256
            );
            std::process::exit(1);
        }

        std::fs::write(out_dir.join(p.out_name), &bytes).unwrap_or_else(|e| {
            eprintln!("error: failed to write {} to OUT_DIR: {e}", p.out_name);
            std::process::exit(1);
        });
    }
}
