//! Stderr-only progress reporting across a run's three phases — reading
//! stdin, loading the model, and transcribing (spec 002-transcription-progress).
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
/// and threaded through to `audio::stage_stdin`, model-session construction,
/// and `inference::engine`'s chunked encode/decode passes.
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

    // ---- Phase: transcribing (FR-002/005/006) ----

    /// `total_duration_secs` is the input's already-known audio duration
    /// (`InputMedia.duration_secs`, probed identically for both input
    /// methods before this phase begins — data-model.md). Represented as
    /// audio-milliseconds internally (research.md §5) since indicatif's
    /// position/length are `u64`.
    pub fn start_transcription(&mut self, total_duration_secs: f64) {
        if self.suppressed {
            return;
        }
        if self.interactive {
            let total_ms = (total_duration_secs * 1000.0).round() as u64;
            let pb = ProgressBar::new(total_ms);
            pb.set_style(
                ProgressStyle::with_template(
                    "transcribing [{bar:40.cyan/blue}] {percent}% (eta: {eta})",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar()),
            );
            self.bar = Some(pb);
        } else {
            eprintln!("transcribing...");
        }
    }

    /// Advances the bar by half of one chunk's audio duration, once that
    /// chunk's *encoding* pass completes. Split 50/50 with
    /// [`Self::advance_decoded`] rather than crediting a chunk's full
    /// duration on encode alone: encode and decode are two separate passes
    /// over every chunk (`encode_chunked` builds all chunks' encoder output
    /// first, then `decode_tdt`/`decode_ctc` decode them), and TDT's
    /// autoregressive decode is frequently the slower half — crediting the
    /// full chunk on encode would make the bar read "done" long before the
    /// process actually finishes. No output in non-interactive mode: the
    /// plain-text fallback only has one milestone worth reporting per
    /// chunk, emitted once that chunk is *fully* transcribed (encode +
    /// decode), not merely encoded — see [`Self::advance_decoded`].
    pub fn advance_encoded(&mut self, chunk_duration_secs: f64) {
        if self.suppressed {
            return;
        }
        if let Some(pb) = &self.bar {
            pb.inc((chunk_duration_secs * 500.0).round() as u64);
        }
    }

    /// Advances the bar by the remaining half of one chunk's audio
    /// duration, once that chunk's *decoding* pass completes. In
    /// non-interactive mode, this is where the plain per-chunk milestone is
    /// emitted (extending spec 001's `"transcribing chunk N of M"` line,
    /// now timed to when the chunk is actually done rather than when it
    /// starts, and emitted uniformly rather than only when chunking was
    /// needed — FR-011).
    pub fn advance_decoded(
        &mut self,
        chunk_index: usize,
        total_chunks: usize,
        chunk_duration_secs: f64,
    ) {
        if self.suppressed {
            return;
        }
        if let Some(pb) = &self.bar {
            pb.inc((chunk_duration_secs * 500.0).round() as u64);
        } else if !self.interactive {
            eprintln!("transcribing chunk {chunk_index} of {total_chunks}");
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
        p.start_transcription(10.0);
        assert!(p.bar.is_none());
    }

    #[test]
    fn interactive_transcription_reaches_full_length_after_encode_and_decode_halves() {
        let mut p = interactive();
        p.start_transcription(10.0);
        let bar = p.bar.as_ref().unwrap();
        assert_eq!(bar.length(), Some(10_000));
        assert_eq!(bar.position(), 0);
        p.advance_encoded(10.0);
        assert_eq!(p.bar.as_ref().unwrap().position(), 5_000);
        p.advance_decoded(1, 1, 10.0);
        assert_eq!(p.bar.as_ref().unwrap().position(), 10_000);
    }

    #[test]
    fn interactive_mode_builds_a_real_bar_for_model_loading_and_stdin() {
        let mut p = interactive();
        p.start_model_loading();
        assert!(p.bar.is_some());
        p.finish_model_loading();
        assert!(p.bar.is_none());

        p.start_reading_stdin();
        assert!(p.bar.is_some());
        p.finish_reading_stdin();
        assert!(p.bar.is_none());
    }

    #[test]
    fn non_interactive_mode_never_builds_a_bar() {
        let mut p = non_interactive();
        p.start_reading_stdin();
        assert!(p.bar.is_none());
        p.start_model_loading();
        assert!(p.bar.is_none());
        p.start_transcription(10.0);
        assert!(p.bar.is_none());
        p.advance_encoded(10.0);
        p.advance_decoded(1, 1, 10.0);
        assert!(p.bar.is_none());
    }
}
