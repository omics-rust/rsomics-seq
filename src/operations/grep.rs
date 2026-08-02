use std::collections::HashSet;
use std::io::Write;

use memchr::memmem;
use rsomics_common::{Result, RsomicsError};
use rsomics_seqio::Format;
use serde::Serialize;

use crate::input::copy_matching_records;
use crate::operations::stats::{SeqType, classify_record};

/// Record field selected by [`grep_records`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GrepMode {
    /// Match the identifier before its first ASCII whitespace.
    Id,
    /// Match the complete FASTA/FASTQ header.
    Name,
    /// Find a literal pattern within the sequence.
    Sequence,
}

/// Options for literal FASTA/FASTQ record filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepOptions {
    /// Literal patterns. ID and name matching use whole-string equality;
    /// sequence matching uses substring search.
    pub patterns: Vec<String>,
    /// Record field to search.
    pub mode: GrepMode,
    /// Compare ASCII letters case-insensitively.
    pub ignore_case: bool,
    /// Keep non-matching records.
    pub invert_match: bool,
    /// Search only the positive strand in sequence mode.
    pub only_positive_strand: bool,
}

/// Summary of one record-filtering pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrepReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected input format.
    pub format: &'static str,
    /// Selected record field.
    pub mode: GrepMode,
    /// Number of distinct literal patterns.
    pub pattern_count: usize,
    /// Whether matching ignored ASCII case.
    pub ignore_case: bool,
    /// Whether the selection was inverted.
    pub invert_match: bool,
    /// Whether sequence matching searched only the positive strand.
    pub only_positive_strand: bool,
    /// Number of input records scanned.
    pub total_records: u64,
    /// Number of records emitted.
    pub matched_records: u64,
}

enum Patterns {
    Exact(HashSet<Vec<u8>>),
    Substring(Vec<Vec<u8>>),
}

impl Patterns {
    fn build(options: &GrepOptions) -> Result<Self> {
        if options.patterns.is_empty() {
            return Err(RsomicsError::InvalidInput(
                "grep requires at least one pattern".into(),
            ));
        }
        if options.only_positive_strand && options.mode != GrepMode::Sequence {
            return Err(RsomicsError::InvalidInput(
                "--only-positive-strand requires --by-seq".into(),
            ));
        }

        let normalize = |pattern: &str| {
            if options.ignore_case {
                pattern.as_bytes().to_ascii_lowercase()
            } else {
                pattern.as_bytes().to_vec()
            }
        };
        let normalized: HashSet<Vec<u8>> = options
            .patterns
            .iter()
            .map(|value| normalize(value))
            .collect();
        if options.mode == GrepMode::Sequence {
            for pattern in &options.patterns {
                if !is_legal_sequence_pattern(pattern.as_bytes()) {
                    return Err(RsomicsError::InvalidInput(format!(
                        "illegal DNA/RNA/protein sequence pattern: {pattern}"
                    )));
                }
            }
            Ok(Self::Substring(normalized.into_iter().collect()))
        } else {
            Ok(Self::Exact(normalized))
        }
    }

    fn matches(&self, target: &[u8], ignore_case: bool) -> bool {
        let lowered;
        let target = if ignore_case {
            lowered = target.to_ascii_lowercase();
            lowered.as_slice()
        } else {
            target
        };
        match self {
            Self::Exact(patterns) => patterns.contains(target),
            Self::Substring(patterns) => patterns.iter().any(|pattern| {
                if pattern.is_empty() {
                    target.is_empty()
                } else {
                    memmem::find(target, pattern).is_some()
                }
            }),
        }
    }
}

/// Filters records while preserving input order and format.
///
/// ID and full-name modes implement SeqKit's literal whole-target semantics.
/// Sequence mode implements literal substring matching and searches both
/// strands for DNA/RNA input unless `only_positive_strand` is set. Protein or
/// unclassified input is searched on the positive strand only.
///
/// # Errors
///
/// Returns an error for an empty pattern set, an illegal sequence pattern,
/// malformed input, count overflow, or output failure.
pub fn grep_records(
    input: &str,
    options: &GrepOptions,
    output: &mut dyn Write,
) -> Result<GrepReport> {
    let patterns = Patterns::build(options)?;
    let pattern_count = match &patterns {
        Patterns::Exact(patterns) => patterns.len(),
        Patterns::Substring(patterns) => patterns.len(),
    };
    let mut sequence_type = None;

    let (format, total_records, matched_records) =
        copy_matching_records(input, output, |record| {
            let sequence_type = *sequence_type.get_or_insert_with(|| classify_record(record.seq));
            let hit = match options.mode {
                GrepMode::Id => patterns.matches(split_id(record.id), options.ignore_case),
                GrepMode::Name => patterns.matches(record.id, options.ignore_case),
                GrepMode::Sequence => sequence_matches(
                    record.seq,
                    &patterns,
                    options.ignore_case,
                    options.only_positive_strand,
                    sequence_type,
                ),
            };
            Ok(hit != options.invert_match)
        })?;

    Ok(GrepReport {
        input: input.to_owned(),
        format: format_name(format),
        mode: options.mode,
        pattern_count,
        ignore_case: options.ignore_case,
        invert_match: options.invert_match,
        only_positive_strand: options.only_positive_strand,
        total_records,
        matched_records,
    })
}

fn split_id(name: &[u8]) -> &[u8] {
    name.split(|byte| byte.is_ascii_whitespace())
        .next()
        .unwrap_or(name)
}

fn sequence_matches(
    sequence: &[u8],
    patterns: &Patterns,
    ignore_case: bool,
    only_positive: bool,
    sequence_type: SeqType,
) -> bool {
    if patterns.matches(sequence, ignore_case) {
        return true;
    }
    if only_positive || matches!(sequence_type, SeqType::Protein | SeqType::Unlimit) {
        return false;
    }
    let reverse = reverse_complement(sequence, sequence_type);
    patterns.matches(&reverse, ignore_case)
}

fn reverse_complement(sequence: &[u8], sequence_type: SeqType) -> Vec<u8> {
    sequence
        .iter()
        .rev()
        .map(|&byte| match byte {
            b'A' => {
                if sequence_type == SeqType::Rna {
                    b'U'
                } else {
                    b'T'
                }
            }
            b'a' => {
                if sequence_type == SeqType::Rna {
                    b'u'
                } else {
                    b't'
                }
            }
            b'T' | b'U' => b'A',
            b't' | b'u' => b'a',
            b'C' => b'G',
            b'c' => b'g',
            b'G' => b'C',
            b'g' => b'c',
            b'R' => b'Y',
            b'r' => b'y',
            b'Y' => b'R',
            b'y' => b'r',
            b'K' => b'M',
            b'k' => b'm',
            b'M' => b'K',
            b'm' => b'k',
            b'B' => b'V',
            b'b' => b'v',
            b'V' => b'B',
            b'v' => b'b',
            b'D' => b'H',
            b'd' => b'h',
            b'H' => b'D',
            b'h' => b'd',
            other => other,
        })
        .collect()
}

fn is_legal_sequence_pattern(pattern: &[u8]) -> bool {
    pattern
        .iter()
        .all(|byte| byte.is_ascii_alphabetic() || matches!(byte, b' ' | b'-' | b'.' | b'*' | b'_'))
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Fasta => "FASTA",
        Format::Fastq => "FASTQ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_first_header_token() {
        assert_eq!(split_id(b"seq1 description"), b"seq1");
    }

    #[test]
    fn reverse_complement_tracks_dna_and_rna() {
        assert_eq!(reverse_complement(b"ACGT", SeqType::Dna), b"ACGT");
        assert_eq!(reverse_complement(b"ACGU", SeqType::Rna), b"ACGU");
        assert_eq!(
            reverse_complement(b"RYSWKMBVDHN", SeqType::Dna),
            b"NDHBVKMWSRY"
        );
    }

    #[test]
    fn empty_sequence_pattern_only_matches_empty_sequence() {
        let patterns = Patterns::Substring(vec![Vec::new()]);
        assert!(patterns.matches(b"", false));
        assert!(!patterns.matches(b"A", false));
    }
}
