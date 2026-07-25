mod audio;
mod inference;
mod model;
mod output;
mod progress;

use anyhow::Context;
use clap::Parser;
use inference::Transcript;
use inference::swift_bridge::SwiftAsrBridge;
use model::registry::ModelEntry;
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

    /// Force CPU-only inference (no Neural Engine/GPU dispatch) — useful for
    /// benchmarking or troubleshooting. Default uses the Neural Engine.
    #[arg(long, default_value = "auto", env = "PARA_DEVICE")]
    device: DeviceArg,

    /// List available models and their cache state, then exit.
    #[arg(long)]
    list_models: bool,

    /// Force a fresh re-download of the selected model's cached files.
    #[arg(long)]
    refresh_model: bool,

    /// Suppress all progress output on stderr (errors are unaffected).
    ///
    /// `PARA_NO_PROGRESS` (any non-empty value) has the same effect — handled
    /// manually in `run()` rather than via clap's `env` on this field, since
    /// clap's bool+env parsing only accepts literal "true"/"false" and
    /// rejects other values (e.g. "1") with a hard error, not the permissive
    /// "any non-empty value" behavior contracts/cli-interface.md documents.
    #[arg(long)]
    no_progress: bool,
}

/// `--device`'s two meaningful values now that inference runs entirely
/// through the native CoreML backend (004-native-coreml-backend) — `coreml`
/// is kept as an accepted synonym for `auto` so existing scripts using it
/// don't break, since there is no separate ONNX Runtime CoreML execution
/// provider left to distinguish it from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
enum DeviceArg {
    #[default]
    Auto,
    Coreml,
    Cpu,
}

impl DeviceArg {
    fn is_cpu_only(self) -> bool {
        matches!(self, DeviceArg::Cpu)
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Checked first, before the input-required check below: --list-models
    // is a standalone query (FR-019) and never needs -i/stdin at all.
    if cli.list_models {
        return model::registry::list_models();
    }

    if cli.input.is_none() && std::io::stdin().is_terminal() {
        anyhow::bail!("no input provided — pass -i <file> or pipe audio to stdin");
    }

    let entry = resolve_model(cli.model.as_deref())?;

    let no_progress =
        cli.no_progress || std::env::var("PARA_NO_PROGRESS").is_ok_and(|v| !v.is_empty());
    let mut progress = progress::TranscriptionProgress::new(no_progress);

    // Open the output destination up front, before any expensive work — an
    // unwritable path (no permission, no such directory) should fail as
    // fast as a bad input path, not only after a full transcription
    // completes (FR-024, Constitution Principle IV).
    let mut output_sink = match &cli.output {
        Some(path) => Some(
            std::fs::File::create(path)
                .with_context(|| format!("cannot write output file: {}", path.display()))?,
        ),
        None => None,
    };

    // Resolve and validate input *before* the (potentially multi-GB) model
    // download — a bad path or unusable file should fail fast (FR-015,
    // Constitution Principle IV) without first waiting on a download the
    // user didn't need. `_stdin_guard` keeps the staged temp file alive
    // until `run()` returns.
    let (input_path, _stdin_guard) = match &cli.input {
        Some(path) => (path.clone(), None),
        None => {
            let staged = audio::stage_stdin(&mut progress)?;
            let path = staged.path().to_path_buf();
            (path, Some(staged))
        }
    };

    // Diagnostics only — an unrecognized header never blocks the run
    // (FR-001's Assumption: ffmpeg gets the final say on whether it can
    // decode a format, not this magic-byte sniff).
    if let Ok(mut file) = std::fs::File::open(&input_path) {
        let mut header = [0u8; 12];
        if std::io::Read::read_exact(&mut file, &mut header).is_ok()
            && audio::detect_format(&header).is_none()
        {
            eprintln!(
                "note: could not identify input format from its header, attempting anyway: {}",
                input_path.display()
            );
        }
    }

    let probe = audio::probe(&input_path)
        .map_err(|e| anyhow::anyhow!("{e:#}"))
        .with_context(|| format!("input not usable: {}", input_path.display()))?;
    if !probe.has_audio_track {
        anyhow::bail!(
            "input has no audio track: {} (FR-015)",
            input_path.display()
        );
    }

    let wav_file = tempfile::NamedTempFile::new()
        .context("failed to create temp file for transcoded audio")?;
    audio::transcode_to_wav(&input_path, wav_file.path())?;

    eprintln!("using model: {}", entry.id);

    let mut bridge = SwiftAsrBridge::new().context("failed to initialize native CoreML backend")?;

    if cli.refresh_model {
        bridge
            .refresh_model(entry.version)
            .with_context(|| format!("failed to refresh model {}", entry.id))?;
    }

    progress.start_model_loading();
    bridge
        .load_model(entry.version, cli.device.is_cpu_only())
        .with_context(|| format!("failed to load model {}", entry.id))?;
    progress.finish_model_loading();

    progress.start_transcription();
    let result = bridge
        .transcribe_file(wav_file.path())
        .context("transcription failed")?;
    progress.finish_transcription();

    let segments = inference::segments::build_segments(&result.words);
    let text = inference::segments::join_as_paragraphs(&segments);
    let transcript = Transcript {
        text,
        segments,
        model: entry.id.to_string(),
        duration_secs: probe.duration_secs,
    };

    write_output(&cli, &transcript, output_sink.as_mut())
}

/// Resolves `--model` (or `PARA_MODEL`) against the registry, or the default
/// model when unset. An unrecognized id fails immediately with the list of
/// valid ids — never a silent substitution (FR-010).
fn resolve_model(requested: Option<&str>) -> anyhow::Result<&'static ModelEntry> {
    match requested {
        None => Ok(model::registry::default_model()),
        Some(id) => model::registry::find(id).ok_or_else(|| {
            let valid: Vec<_> = model::registry::MODELS.iter().map(|m| m.id).collect();
            anyhow::anyhow!("unknown model {id:?} — valid options: {}", valid.join(", "))
        }),
    }
}

/// Writes `transcript` to stdout or `-o <file>` (FR-011). A file that can't
/// be created/written (no permission, no disk space) fails loud with a
/// specific error rather than a silent partial write (FR-024).
fn write_output(
    cli: &Cli,
    transcript: &Transcript,
    output_sink: Option<&mut std::fs::File>,
) -> anyhow::Result<()> {
    match output_sink {
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            output::write_transcript(cli.format, transcript, &mut lock)
        }
        Some(file) => {
            let path = cli
                .output
                .as_deref()
                .expect("output_sink implies -o was set");
            output::write_transcript(cli.format, transcript, file)
                .with_context(|| format!("failed writing output file: {}", path.display()))
        }
    }
}
