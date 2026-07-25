//! Stderr-only progress reporting across a run's three phases — reading
//! stdin, loading the model, and transcribing (spec 002-transcription-progress).
//!
//! The native CoreML backend (004-native-coreml-backend) transcribes a file
//! in one call with no per-chunk callback, so — unlike the old ONNX Runtime
//! pipeline — there is no known total to show a percentage/ETA against;
//! every phase here is an indeterminate spinner (or a single plain-text
//! line when non-interactive), not a progress bar.
//!
//! indicatif's own default behavior on a non-interactive stderr (redirected,
//! or `TERM` unset/`dumb`) is to go fully silent, not degrade to plain text
//! (research.md §1) — so this module makes its own `is_interactive` check
//! and explicitly branches, rather than relying on indicatif's default
//! `ProgressDrawTarget::stderr()` hiding behavior.

use indicatif::{ProgressBar, ProgressStyle};

/// Whether stderr is an interactive terminal capable of an animated
/// indicator. Reuses `console`'s own terminal/dumb detection (research.md
/// §2-3) rather than reimplementing it — `console::is_dumb()`'s Unix default
/// (an *unset* `TERM` counts as dumb) is easy to get wrong by hand.
fn is_interactive() -> bool {
    console::Term::stderr().is_term() && !console::is_dumb()
}

/// One progress-reporting handle per run, constructed once in `main::run()`
/// and threaded through to `audio::stage_stdin` and the model-load/
/// transcribe phases.
pub struct TranscriptionProgress {
    suppressed: bool,
    interactive: bool,
    bar: Option<ProgressBar>,
}

impl TranscriptionProgress {
    /// `suppressed` is `--no-progress`/`PARA_NO_PROGRESS` (FR-008) — when
    /// true, every method below is a no-op.
    pub fn new(suppressed: bool) -> Self {
        Self {
            suppressed,
            interactive: is_interactive(),
            bar: None,
        }
    }

    fn spinner(&self, message: &str) -> Option<ProgressBar> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    }

    // ---- Phase: acquiring input (stdin only; -i's known file size makes
    // this phase effectively instantaneous and not worth a separate
    // indicator, research.md's Context) ----

    /// FR-004: stdin's total size isn't known upfront, so this is always an
    /// indeterminate indicator, never a percentage.
    pub fn start_reading_stdin(&mut self) {
        if self.suppressed {
            return;
        }
        if self.interactive {
            self.bar = self.spinner("reading stdin...");
        } else {
            eprintln!("reading stdin...");
        }
    }

    pub fn update_bytes_read(&mut self, bytes: u64) {
        if self.suppressed {
            return;
        }
        if let Some(pb) = &self.bar {
            pb.set_message(format!("reading stdin... {}", indicatif::HumanBytes(bytes)));
        }
        // Non-interactive: no periodic milestone here — a single start
        // line is enough to prove the tool is working without flooding a
        // redirected log with byte counts (FR-007's "plain milestones",
        // not "a line per read").
    }

    pub fn finish_reading_stdin(&mut self) {
        if let Some(pb) = self.bar.take() {
            pb.finish_and_clear();
        }
    }

    // ---- Phase: loading the model (FR-003 — every run, any input length) ----

    pub fn start_model_loading(&mut self) {
        if self.suppressed {
            return;
        }
        if self.interactive {
            self.bar = self.spinner("loading model...");
        } else {
            eprintln!("loading model...");
        }
    }

    pub fn finish_model_loading(&mut self) {
        if let Some(pb) = self.bar.take() {
            pb.finish_and_clear();
        }
    }

    // ---- Phase: transcribing ----

    /// The native CoreML backend has no per-chunk callback to drive an
    /// incremental bar with, so this is always an indeterminate spinner
    /// (or a single plain-text line non-interactively) regardless of input
    /// length.
    pub fn start_transcription(&mut self) {
        if self.suppressed {
            return;
        }
        if self.interactive {
            self.bar = self.spinner("transcribing...");
        } else {
            eprintln!("transcribing...");
        }
    }

    pub fn finish_transcription(&mut self) {
        if let Some(pb) = self.bar.take() {
            pb.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suppressed() -> TranscriptionProgress {
        TranscriptionProgress {
            suppressed: true,
            interactive: true,
            bar: None,
        }
    }

    fn interactive() -> TranscriptionProgress {
        TranscriptionProgress {
            suppressed: false,
            interactive: true,
            bar: None,
        }
    }

    fn non_interactive() -> TranscriptionProgress {
        TranscriptionProgress {
            suppressed: false,
            interactive: false,
            bar: None,
        }
    }

    #[test]
    fn suppressed_never_creates_a_bar() {
        let mut p = suppressed();
        p.start_reading_stdin();
        assert!(p.bar.is_none());
        p.start_model_loading();
        assert!(p.bar.is_none());
        p.start_transcription();
        assert!(p.bar.is_none());
    }

    #[test]
    fn interactive_mode_builds_a_real_bar_for_every_phase() {
        let mut p = interactive();
        p.start_model_loading();
        assert!(p.bar.is_some());
        p.finish_model_loading();
        assert!(p.bar.is_none());

        p.start_reading_stdin();
        assert!(p.bar.is_some());
        p.finish_reading_stdin();
        assert!(p.bar.is_none());

        p.start_transcription();
        assert!(p.bar.is_some());
        p.finish_transcription();
        assert!(p.bar.is_none());
    }

    #[test]
    fn non_interactive_mode_never_builds_a_bar() {
        let mut p = non_interactive();
        p.start_reading_stdin();
        assert!(p.bar.is_none());
        p.start_model_loading();
        assert!(p.bar.is_none());
        p.start_transcription();
        assert!(p.bar.is_none());
    }
}
