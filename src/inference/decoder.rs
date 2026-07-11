use anyhow::Context;
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
    pub fn decode(&self, token_ids: &[i64]) -> String {
        let mut joined = String::new();
        for &id in token_ids {
            if let Some(piece) = self.pieces.get(id as usize) {
                joined.push_str(piece);
            }
        }
        joined.replace(WORD_BOUNDARY, " ").trim_start().to_string()
    }
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
}
