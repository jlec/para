use crate::inference::engine::{CtcOutput, EncoderOutput};
use crate::inference::{Segment, Transcript};
use anyhow::Context;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// The SentencePiece word-boundary marker used by this model family's vocab.
const WORD_BOUNDARY: char = '▁';

/// Name of the blank token used by TDT/CTC decode loops (not part of emitted text).
const BLANK_TOKEN: &str = "<blk>";

/// Loaded `vocab.txt`: a line-indexed SentencePiece piece list (research.md §10,
/// including the 2026-07-11 correction on the file's actual `<piece> <id>` line format).
pub struct Vocab {
    pieces: Vec<String>,
    blank_id: Option<usize>,
}

impl Vocab {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read vocab file {}", path.display()))?;

        let mut pieces = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            let (piece, id_str) = line.rsplit_once(' ').with_context(|| {
                format!(
                    "{}: line {} missing trailing id field: {line:?}",
                    path.display(),
                    line_no + 1
                )
            })?;
            let id: usize = id_str.parse().with_context(|| {
                format!(
                    "{}: line {} has non-numeric id: {line:?}",
                    path.display(),
                    line_no + 1
                )
            })?;
            anyhow::ensure!(
                id == line_no,
                "{}: line {} has id {id}, expected {line_no} (piece ids must match line order)",
                path.display(),
                line_no + 1
            );
            pieces.push(piece.to_string());
        }

        let blank_id = pieces.iter().position(|p| p == BLANK_TOKEN);
        Ok(Self { pieces, blank_id })
    }

    /// Id of the blank token, if this vocab has one (used by the TDT/CTC decode loops
    /// to exclude blank emissions from the output text).
    pub fn blank_id(&self) -> Option<usize> {
        self.blank_id
    }

    /// Converts a sequence of non-blank token ids into text: look up each id's piece,
    /// concatenate, then replace the SentencePiece word-boundary marker with a space.
    /// Runs of 2+ spaces collapse to one (can happen if two isolated `▁`-only
    /// pieces are emitted back to back) — a simplified stand-in for the
    /// reference implementation's regex-based space cleanup (research.md's
    /// TDT-decode addendum), covering the common case without porting the
    /// exact regex.
    pub fn decode(&self, token_ids: &[i64]) -> String {
        let mut joined = String::new();
        for &id in token_ids {
            if let Some(piece) = self.pieces.get(id as usize) {
                joined.push_str(piece);
            }
        }
        let collapsed = joined.replace(WORD_BOUNDARY, " ");
        collapsed.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Number of entries in this vocab (used to split TDT joint-network
    /// output into token logits vs. duration logits).
    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

/// Prediction-network (decoder) LSTM hidden size, verified against both real
/// `decoder_joint-model.onnx` files in the registry (parakeet-tdt-0.6b-v3 and
/// -v2) — vocab-independent, unlike the joint network's output size.
const PREDNET_HIDDEN: usize = 640;

/// Number of TDT duration-head classes (0..=4 encoder frames to skip),
/// verified against both real decoder_joint-model.onnx files: the joint
/// output's last dimension is always `vocab_size + 5`, never a fixed
/// absolute total (vocab size itself differs: 8193 for -v3, 1025 for -v2).
const NUM_DURATION_BINS: usize = 5;

/// Safety cap on consecutive non-advancing emissions at a single encoder
/// frame, matching the reference `onnx-asr` implementation's
/// `max_tokens_per_step` default — without it a degenerate duration-0 loop
/// could in principle spin forever on one frame.
const MAX_TOKENS_PER_STEP: usize = 10;

/// Minimum log-probability margin a CTC frame's winning non-blank token must
/// hold over blank to be accepted, rather than treated as blank (research.md
/// §16). Measured empirically against the real `parakeet-ctc-0.6b` model, not
/// guessed (Constitution Principle V): isolated single-frame hallucinations on
/// out-of-distribution pure-digital-silence input showed margins of 0.62,
/// 1.26, and 2.14; genuine spoken tokens (including the softest real
/// disfluency observed, a synthesized "um") never dropped below 3.14, and
/// crisp phonemes typically exceed 7. 2.5 sits with real margin on both
/// sides of every sample measured.
const MIN_BLANK_MARGIN: f32 = 2.5;

/// Encoder-frame duration in seconds: the bundled preprocessor emits mel
/// frames at 100/sec (10ms hop, measured directly — research.md §6), and the
/// encoder subsamples by 8x, so each encoder-output frame spans 80ms.
const ENCODER_FRAME_SECONDS: f64 = 0.08;

/// Minimum silence gap (in seconds) between two emitted tokens' encoder
/// frames before starting a new phrase-level segment. Not specified by the
/// reference `onnx-asr` implementation (its public API only returns flat
/// token-level timestamps, no segment grouping) — this project's own design
/// decision for data-model.md's "phrase/sentence, not word-level" Segment
/// requirement (research.md's TDT-decode addendum).
const SEGMENT_GAP_SECONDS: f64 = 1.5;

/// One emitted (non-blank) token and the encoder frame it was emitted at.
struct TokenTiming {
    token_id: usize,
    /// Global encoder-frame index (post 8x subsampling), including any
    /// chunk offset — convert to seconds via `frame * ENCODER_FRAME_SECONDS`.
    frame: usize,
}

fn argmax(xs: &[f32]) -> usize {
    xs.iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .expect("logits slice must be non-empty")
}

/// Gathers encoder frame `t`'s `hidden_size` values into a contiguous
/// buffer. `EncoderOutput.data` is flattened row-major as `[1, hidden, T]`,
/// so a fixed time step is strided (one value every `frames` elements), not
/// contiguous — this copies it into the `[1, hidden, 1]` shape the
/// decoder-joint network expects per step.
fn frame_slice(encoder_output: &EncoderOutput, t: usize) -> Vec<f32> {
    (0..encoder_output.hidden_size)
        .map(|h| encoder_output.data[h * encoder_output.frames + t])
        .collect()
}

/// The prediction network's autoregressive state, carried across chunk
/// boundaries by [`decode_tdt`] (research.md §17) rather than reset per
/// chunk: chunking exists only to bound single-pass encoder memory/latency
/// (research.md §6), not to mark separate utterances, so the decoder's LSTM
/// state and previous-token context should persist across a chunk split the
/// same way it does across any other encoder frame.
struct DecoderState {
    state1: Vec<f32>,
    state2: Vec<f32>,
    prev_token: i32,
}

impl DecoderState {
    fn new(blank_id: usize) -> Self {
        Self {
            state1: vec![0.0f32; 2 * PREDNET_HIDDEN],
            state2: vec![0.0f32; 2 * PREDNET_HIDDEN],
            prev_token: blank_id as i32,
        }
    }
}

/// Greedy TDT decode for one chunk's encoder output, following the reference
/// `onnx-asr` `NemoConformerTdt`/`_AsrWithTransducerDecoding` algorithm
/// (verified against the real Python source on GitHub, not re-derived from
/// tensor shapes alone — Constitution Principle V). `state` is owned by the
/// caller ([`decode_tdt`]) and carried across chunks, not reset per call.
fn decode_tdt_chunk(
    encoder_output: &EncoderOutput,
    decoder_joint: &mut Session,
    vocab: &Vocab,
    frame_offset: usize,
    state: &mut DecoderState,
) -> anyhow::Result<Vec<TokenTiming>> {
    let blank_id = vocab
        .blank_id()
        .context("vocab has no blank token; TDT decode requires one")?;
    let vocab_size = vocab.len();

    let mut out = Vec::new();
    let mut t = 0usize;
    let mut emitted_at_t = 0usize;

    while t < encoder_output.frames {
        let frame = frame_slice(encoder_output, t);
        let encoder_outputs =
            Tensor::from_array(([1usize, encoder_output.hidden_size, 1usize], frame))?;
        let targets = Tensor::from_array(([1usize, 1usize], vec![state.prev_token]))?;
        let target_length = Tensor::from_array(([1usize], vec![1i32]))?;
        let input_states_1 =
            Tensor::from_array(([2usize, 1usize, PREDNET_HIDDEN], state.state1.clone()))?;
        let input_states_2 =
            Tensor::from_array(([2usize, 1usize, PREDNET_HIDDEN], state.state2.clone()))?;

        let outputs = decoder_joint.run(ort::inputs![
            "encoder_outputs" => encoder_outputs,
            "targets" => targets,
            "target_length" => target_length,
            "input_states_1" => input_states_1,
            "input_states_2" => input_states_2,
        ])?;

        let (_, joint_slice) = outputs["outputs"].try_extract_tensor::<f32>()?;
        let joint = joint_slice.to_vec();
        anyhow::ensure!(
            joint.len() == vocab_size + NUM_DURATION_BINS,
            "decoder_joint output size {} does not match vocab_size {vocab_size} + {NUM_DURATION_BINS} duration bins",
            joint.len()
        );
        let token = argmax(&joint[..vocab_size]);
        let duration = argmax(&joint[vocab_size..]);

        if token != blank_id {
            let (_, new_state1) = outputs["output_states_1"].try_extract_tensor::<f32>()?;
            let (_, new_state2) = outputs["output_states_2"].try_extract_tensor::<f32>()?;
            state.state1 = new_state1.to_vec();
            state.state2 = new_state2.to_vec();
            state.prev_token = token as i32;
            out.push(TokenTiming {
                token_id: token,
                frame: frame_offset + t,
            });
            emitted_at_t += 1;
        }

        if duration > 0 {
            t += duration;
            emitted_at_t = 0;
        } else if token == blank_id || emitted_at_t >= MAX_TOKENS_PER_STEP {
            t += 1;
            emitted_at_t = 0;
        }
    }

    Ok(out)
}

/// Runs TDT greedy decode across every chunk from `engine::encode_chunked`,
/// then groups the flat token stream into phrase-level segments and builds
/// the final `Transcript` (data-model.md's `Segment` timing granularity).
/// The decoder's autoregressive state carries across chunk boundaries
/// (research.md §17) — chunking is a single-pass-encoder memory/latency
/// bound (research.md §6), not an utterance boundary.
pub fn decode_tdt(
    chunks: &[EncoderOutput],
    decoder_joint: &mut Session,
    vocab: &Vocab,
    model_id: &str,
    duration_secs: f64,
    progress: &mut crate::progress::TranscriptionProgress,
) -> anyhow::Result<Transcript> {
    let blank_id = vocab
        .blank_id()
        .context("vocab has no blank token; TDT decode requires one")?;
    let mut state = DecoderState::new(blank_id);
    let mut tokens = Vec::new();
    let mut frame_offset = 0usize;
    let total_chunks = chunks.len();
    for (i, chunk) in chunks.iter().enumerate() {
        tokens.extend(decode_tdt_chunk(
            chunk,
            decoder_joint,
            vocab,
            frame_offset,
            &mut state,
        )?);
        frame_offset += chunk.frames;
        progress.advance_decoded(
            i + 1,
            total_chunks,
            chunk.frames as f64 * ENCODER_FRAME_SECONDS,
        );
    }

    let text = vocab.decode(&tokens.iter().map(|t| t.token_id as i64).collect::<Vec<_>>());
    let segments = group_into_segments(&tokens, vocab, duration_secs);
    Ok(Transcript {
        text,
        segments,
        model: model_id.to_string(),
        duration_secs,
    })
}

/// Greedy CTC decode: per-frame argmax, then collapse consecutive repeats
/// and drop blanks (reference `onnx-asr` `_AsrWithCtcDecoding._decoding`).
fn decode_ctc_chunk(
    logprobs: &CtcOutput,
    vocab: &Vocab,
    frame_offset: usize,
) -> anyhow::Result<Vec<TokenTiming>> {
    let blank_id = vocab
        .blank_id()
        .context("vocab has no blank token; CTC decode requires one")?;

    let mut out = Vec::new();
    let mut prev_token = blank_id;
    for t in 0..logprobs.frames {
        let frame = &logprobs.data[t * logprobs.vocab_size..(t + 1) * logprobs.vocab_size];
        let mut token = argmax(frame);
        if token != blank_id && frame[token] - frame[blank_id] < MIN_BLANK_MARGIN {
            // Below the confidence margin real speech tokens hold over blank
            // (research.md §16) — likely an isolated low-confidence
            // hallucination on silent/near-silent input (FR-015's "no
            // detectable speech" edge case), not a genuine token.
            token = blank_id;
        }
        if token != blank_id && token != prev_token {
            out.push(TokenTiming {
                token_id: token,
                frame: frame_offset + t,
            });
        }
        prev_token = token;
    }
    Ok(out)
}

/// Runs CTC greedy decode across every chunk, joining all chunks' text into
/// a single whole-file segment (data-model.md: CTC-tier models have no
/// word/segment-level timing, `TimingGranularity::WholeFile` — one segment
/// spanning `0..duration_secs`, or none at all for a silent/empty input).
pub fn decode_ctc(
    chunks: &[CtcOutput],
    vocab: &Vocab,
    model_id: &str,
    duration_secs: f64,
    progress: &mut crate::progress::TranscriptionProgress,
) -> anyhow::Result<Transcript> {
    let mut tokens = Vec::new();
    let mut frame_offset = 0usize;
    let total_chunks = chunks.len();
    for (i, chunk) in chunks.iter().enumerate() {
        tokens.extend(decode_ctc_chunk(chunk, vocab, frame_offset)?);
        frame_offset += chunk.frames;
        progress.advance_decoded(
            i + 1,
            total_chunks,
            chunk.frames as f64 * ENCODER_FRAME_SECONDS,
        );
    }

    let text = vocab.decode(&tokens.iter().map(|t| t.token_id as i64).collect::<Vec<_>>());
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![Segment {
            start: 0.0,
            end: duration_secs,
            text: text.clone(),
        }]
    };
    Ok(Transcript {
        text,
        segments,
        model: model_id.to_string(),
        duration_secs,
    })
}

/// Groups a flat, time-ordered token stream into phrase-level segments,
/// splitting wherever the gap between two consecutive tokens' encoder frames
/// exceeds `SEGMENT_GAP_SECONDS`. Segments are contiguous (segment `i`'s end
/// equals segment `i+1`'s start) and therefore always ordered and
/// non-overlapping (US4 acceptance scenario 2).
fn group_into_segments(tokens: &[TokenTiming], vocab: &Vocab, duration_secs: f64) -> Vec<Segment> {
    if tokens.is_empty() {
        return vec![];
    }

    let gap_frames = (SEGMENT_GAP_SECONDS / ENCODER_FRAME_SECONDS) as usize;
    let mut boundaries = vec![0];
    for i in 1..tokens.len() {
        if tokens[i].frame.saturating_sub(tokens[i - 1].frame) > gap_frames {
            boundaries.push(i);
        }
    }
    boundaries.push(tokens.len());

    boundaries
        .windows(2)
        .map(|w| {
            let (start_idx, end_idx) = (w[0], w[1]);
            let chunk = &tokens[start_idx..end_idx];
            let text = vocab.decode(&chunk.iter().map(|t| t.token_id as i64).collect::<Vec<_>>());
            let start = chunk[0].frame as f64 * ENCODER_FRAME_SECONDS;
            let end = if end_idx < tokens.len() {
                tokens[end_idx].frame as f64 * ENCODER_FRAME_SECONDS
            } else {
                duration_secs.max(start + ENCODER_FRAME_SECONDS)
            };
            Segment { start, end, text }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_vocab(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    #[test]
    fn decodes_pieces_and_replaces_word_boundary() {
        let file = write_vocab(&["▁hello 0", "▁world 1", "<blk> 2"]);
        let vocab = Vocab::load(file.path()).unwrap();
        assert_eq!(vocab.decode(&[0, 1]), "hello world");
    }

    #[test]
    fn locates_blank_token_by_name() {
        let file = write_vocab(&["▁a 0", "▁b 1", "<blk> 2"]);
        let vocab = Vocab::load(file.path()).unwrap();
        assert_eq!(vocab.blank_id(), Some(2));
    }

    #[test]
    fn no_blank_token_returns_none() {
        let file = write_vocab(&["▁a 0", "▁b 1"]);
        let vocab = Vocab::load(file.path()).unwrap();
        assert_eq!(vocab.blank_id(), None);
    }

    #[test]
    fn rejects_mismatched_line_and_id() {
        let file = write_vocab(&["▁a 0", "▁b 5"]);
        assert!(Vocab::load(file.path()).is_err());
    }

    #[test]
    fn rejects_line_without_id_field() {
        let file = write_vocab(&["justapiece"]);
        assert!(Vocab::load(file.path()).is_err());
    }

    #[test]
    fn unknown_token_id_is_skipped_not_panicking() {
        let file = write_vocab(&["▁a 0"]);
        let vocab = Vocab::load(file.path()).unwrap();
        assert_eq!(vocab.decode(&[0, 99]), "a");
    }

    #[test]
    fn collapses_double_spaces_from_adjacent_word_boundary_pieces() {
        let file = write_vocab(&["▁ 0", "▁hi 1"]);
        let vocab = Vocab::load(file.path()).unwrap();
        assert_eq!(vocab.decode(&[0, 1]), "hi");
    }

    fn word_vocab() -> Vocab {
        let file = write_vocab(&["▁one 0", "▁two 1", "▁three 2", "<blk> 3"]);
        Vocab::load(file.path()).unwrap()
    }

    #[test]
    fn empty_tokens_produce_no_segments() {
        let vocab = word_vocab();
        assert_eq!(group_into_segments(&[], &vocab, 10.0), vec![]);
    }

    #[test]
    fn contiguous_tokens_form_a_single_segment() {
        let vocab = word_vocab();
        let tokens = vec![
            TokenTiming {
                token_id: 0,
                frame: 0,
            },
            TokenTiming {
                token_id: 1,
                frame: 5,
            },
        ];
        let segments = group_into_segments(&tokens, &vocab, 10.0);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "one two");
        assert_eq!(segments[0].start, 0.0);
        assert_eq!(segments[0].end, 10.0);
    }

    #[test]
    fn large_gap_starts_a_new_ordered_non_overlapping_segment() {
        let vocab = word_vocab();
        let gap_frames = (SEGMENT_GAP_SECONDS / ENCODER_FRAME_SECONDS) as usize + 1;
        let tokens = vec![
            TokenTiming {
                token_id: 0,
                frame: 0,
            },
            TokenTiming {
                token_id: 1,
                frame: gap_frames,
            },
            TokenTiming {
                token_id: 2,
                frame: gap_frames + 1,
            },
        ];
        let segments = group_into_segments(&tokens, &vocab, 20.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "one");
        assert_eq!(segments[1].text, "two three");
        // Ordered, non-overlapping, and each end > start (US4 acceptance scenario 2).
        for s in &segments {
            assert!(s.end > s.start);
        }
        assert_eq!(segments[0].end, segments[1].start);
        assert_eq!(segments[1].end, 20.0);
    }

    #[test]
    fn ctc_decode_collapses_repeats_and_drops_blanks() {
        // vocab: 0="one", 1="two", 2=<blk>
        let file = write_vocab(&["▁one 0", "▁two 1", "<blk> 2"]);
        let vocab = Vocab::load(file.path()).unwrap();
        let vocab_size = 3;
        // frames: one, one (repeat, collapsed), blank, two -> "one two"
        // Margins well above MIN_BLANK_MARGIN so the confidence filter
        // doesn't interfere with this test's collapse/drop-blank behavior.
        #[rustfmt::skip]
        let data = vec![
            5.0, 0.0, 0.0, // frame 0: argmax "one"
            5.0, 0.0, 0.0, // frame 1: argmax "one" (repeat of prev, dropped)
            0.0, 0.0, 5.0, // frame 2: argmax blank
            0.0, 5.0, 0.0, // frame 3: argmax "two"
        ];
        let logprobs = CtcOutput {
            data,
            frames: 4,
            vocab_size,
        };
        let tokens = decode_ctc_chunk(&logprobs, &vocab, 0).unwrap();
        let ids: Vec<usize> = tokens.iter().map(|t| t.token_id).collect();
        assert_eq!(ids, vec![0, 1]);
    }

    #[test]
    fn ctc_decode_suppresses_low_margin_token_as_blank() {
        // vocab: 0="uh", 1=<blk>. Winning token's margin over blank (0.3) is
        // below MIN_BLANK_MARGIN — this is the shape of the real
        // hallucination measured on out-of-distribution silent input
        // (research.md §16): a low-confidence non-blank argmax that should be
        // treated as blank, not emitted as a real token.
        let file = write_vocab(&["▁uh 0", "<blk> 1"]);
        let vocab = Vocab::load(file.path()).unwrap();
        let data = vec![0.3, 0.0];
        let logprobs = CtcOutput {
            data,
            frames: 1,
            vocab_size: 2,
        };
        let tokens = decode_ctc_chunk(&logprobs, &vocab, 0).unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn ctc_decode_accepts_token_at_the_margin_boundary() {
        // Same shape as above but with a margin just over MIN_BLANK_MARGIN —
        // confirms the filter doesn't suppress genuinely confident tokens.
        let file = write_vocab(&["▁uh 0", "<blk> 1"]);
        let vocab = Vocab::load(file.path()).unwrap();
        let data = vec![MIN_BLANK_MARGIN + 0.1, 0.0];
        let logprobs = CtcOutput {
            data,
            frames: 1,
            vocab_size: 2,
        };
        let tokens = decode_ctc_chunk(&logprobs, &vocab, 0).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_id, 0);
    }
}
