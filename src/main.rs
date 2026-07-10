// TODO(T024): remove once run() wires audio/model/inference/output together
// into a full transcription pipeline (User Story 1) — until then, most
// Foundational-phase functions have no caller yet, which is expected, not a
// bug (tasks.md's own Foundational checkpoint: "no transcript can be
// produced yet").
#![allow(dead_code)]

mod audio;
mod inference;
mod model;
mod output;

use clap::Parser;
use inference::Device;
use output::OutputFormat;
use std::io::IsTerminal;
use std::path::PathBuf;

/// para — local, offline audio/video transcription.
#[derive(Parser, Debug)]
#[command(
    name = "para",
    version,
    about = "Audio transcription powered by NVIDIA Parakeet"
)]
struct Cli {
    /// Input file. Omit to read from stdin.
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Output file. Omit to write to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Model to use for transcription.
    #[arg(short, long, env = "PARA_MODEL")]
    model: Option<String>,

    /// Output form: text, json, or srt.
    #[arg(short, long, default_value = "text", env = "PARA_FORMAT")]
    format: OutputFormat,

    /// Execution provider: auto, coreml, or cpu.
    #[arg(long, default_value = "auto", env = "PARA_DEVICE")]
    device: Device,

    /// Override the model cache directory.
    #[arg(long, env = "PARA_CACHE_DIR")]
    cache_dir: Option<PathBuf>,

    /// List available models and their cache state, then exit.
    #[arg(long)]
    list_models: bool,

    /// Force a fresh re-download of the selected model's cached files.
    #[arg(long)]
    refresh_model: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.input.is_none() && std::io::stdin().is_terminal() {
        anyhow::bail!("no input provided — pass -i <file> or pipe audio to stdin");
    }

    if cli.list_models {
        return model::registry::list_models();
    }

    // Full transcription wiring (input -> audio -> model -> engine -> decoder
    // -> output) lands in the User Story 1 implementation phase (T022-T025);
    // the Foundational phase only needs each piece to compile and unit-test
    // in isolation.
    let _ = (cli.output, cli.model, cli.cache_dir, cli.refresh_model);
    anyhow::bail!("transcription pipeline not yet wired up (Foundational phase in progress)")
}
