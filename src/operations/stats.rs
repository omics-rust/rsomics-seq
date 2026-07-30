use std::io::Write;

use rsomics_common::{Result, RsomicsError};
use rsomics_seqio::Format;
use serde::Serialize;

use crate::input::scan_records;

const ALPHABET_GUESS_BASES: usize = 10_000;

/// SeqKit-compatible first-record alphabet classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SeqType {
    /// DNA or nucleotide ambiguity alphabet containing thymine.
    #[serde(rename = "DNA")]
    Dna,
    /// Nucleotide alphabet containing uracil and no thymine.
    #[serde(rename = "RNA")]
    Rna,
    /// Protein alphabet.
    Protein,
    /// Input outside the recognized biological alphabets.
    Unlimit,
}

impl SeqType {
    /// Returns the stable tabular spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dna => "DNA",
            Self::Rna => "RNA",
            Self::Protein => "Protein",
            Self::Unlimit => "Unlimit",
        }
    }
}

/// Basic statistics for one input stream.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatsRow {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub file: String,
    /// Detected input format.
    pub format: &'static str,
    /// Alphabet inferred from the first record.
    #[serde(rename = "type")]
    pub seq_type: SeqType,
    /// Number of records.
    pub num_seqs: u64,
    /// Sum of sequence lengths.
    pub sum_len: u64,
    /// Minimum sequence length.
    pub min_len: u64,
    /// Arithmetic mean sequence length.
    pub avg_len: f64,
    /// Maximum sequence length.
    pub max_len: u64,
}

/// Result of the `stats` operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatsReport {
    /// One row per input, preserving command-line order.
    pub rows: Vec<StatsRow>,
}

#[derive(Default)]
struct Accumulator {
    seq_type: Option<SeqType>,
    num_seqs: u64,
    sum_len: u64,
    min_len: u64,
    max_len: u64,
}

impl Accumulator {
    fn push(&mut self, sequence: &[u8]) -> Result<()> {
        let len = u64::try_from(sequence.len()).map_err(|_| {
            RsomicsError::InvalidInput("sequence length cannot be represented as u64".into())
        })?;
        if self.seq_type.is_none() {
            self.seq_type = Some(classify(
                &sequence[..sequence.len().min(ALPHABET_GUESS_BASES)],
            ));
            self.min_len = len;
        } else {
            self.min_len = self.min_len.min(len);
        }
        self.num_seqs = self.num_seqs.checked_add(1).ok_or_else(|| {
            RsomicsError::InvalidInput("record count exceeds u64 capacity".into())
        })?;
        self.sum_len = self.sum_len.checked_add(len).ok_or_else(|| {
            RsomicsError::InvalidInput("total sequence length exceeds u64 capacity".into())
        })?;
        self.max_len = self.max_len.max(len);
        Ok(())
    }

    #[allow(clippy::cast_precision_loss)]
    fn finish(self, file: &str, format: Format) -> Result<StatsRow> {
        let Some(seq_type) = self.seq_type else {
            return Err(RsomicsError::InvalidInput(format!(
                "{file} contains no sequence records"
            )));
        };
        Ok(StatsRow {
            file: file.to_owned(),
            format: format_name(format),
            seq_type,
            num_seqs: self.num_seqs,
            sum_len: self.sum_len,
            min_len: self.min_len,
            avg_len: self.sum_len as f64 / self.num_seqs as f64,
            max_len: self.max_len,
        })
    }
}

/// Computes basic SeqKit-compatible statistics for each input.
///
/// `-` selects stdin and cannot be combined with another input because stdin
/// is a single stream.
///
/// # Errors
///
/// Returns an error when no input is supplied, stdin is combined with another
/// input, a stream is unreadable or malformed, or an aggregate overflows.
pub fn compute_stats(inputs: &[String]) -> Result<StatsReport> {
    if inputs.is_empty() {
        return Err(RsomicsError::InvalidInput(
            "stats requires at least one input".into(),
        ));
    }
    if inputs.len() > 1 && inputs.iter().any(|input| input == "-") {
        return Err(RsomicsError::InvalidInput(
            "stdin (`-`) cannot be combined with file inputs".into(),
        ));
    }

    let mut rows = Vec::with_capacity(inputs.len());
    for input in inputs {
        let mut accumulator = Accumulator::default();
        let format = scan_records(input, |record| accumulator.push(record.seq))?;
        rows.push(accumulator.finish(input, format)?);
    }
    Ok(StatsReport { rows })
}

/// Writes the stable SeqKit-compatible basic TSV table.
///
/// # Errors
///
/// Returns an error when the output cannot be written or flushed.
pub fn write_stats_tsv(report: &StatsReport, output: &mut dyn Write) -> Result<()> {
    writeln!(
        output,
        "file\tformat\ttype\tnum_seqs\tsum_len\tmin_len\tavg_len\tmax_len"
    )
    .map_err(RsomicsError::Io)?;
    for row in &report.rows {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{}",
            row.file,
            row.format,
            row.seq_type.as_str(),
            row.num_seqs,
            row.sum_len,
            row.min_len,
            row.avg_len,
            row.max_len
        )
        .map_err(RsomicsError::Io)?;
    }
    output.flush().map_err(RsomicsError::Io)
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Fasta => "FASTA",
        Format::Fastq => "FASTQ",
    }
}

fn classify(sample: &[u8]) -> SeqType {
    if sample.is_empty() {
        return SeqType::Unlimit;
    }
    const DNA: &[u8] = b"ACGTN -.";
    const RNA: &[u8] = b"ACGUN -.";
    const DNA_REDUNDANT: &[u8] = b"ACGTRYSWKMBDHVN -.";
    const RNA_REDUNDANT: &[u8] = b"ACGURYSWKMBDHVN -.";
    const PROTEIN: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ -*_.";

    let is_subset = |alphabet: &[u8]| {
        sample
            .iter()
            .all(|byte| alphabet.contains(&byte.to_ascii_uppercase()))
    };
    if is_subset(DNA) || is_subset(DNA_REDUNDANT) {
        SeqType::Dna
    } else if is_subset(RNA) || is_subset(RNA_REDUNDANT) {
        SeqType::Rna
    } else if is_subset(PROTEIN) {
        SeqType::Protein
    } else {
        SeqType::Unlimit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_matches_pinned_seqkit_cases() {
        assert_eq!(classify(b"ACGTNRYS"), SeqType::Dna);
        assert_eq!(classify(b"ACGUNRYS"), SeqType::Rna);
        assert_eq!(classify(b"MEEPSILQ"), SeqType::Protein);
        assert_eq!(classify(b"TU"), SeqType::Protein);
        assert_eq!(classify(b"OJ"), SeqType::Protein);
        assert_eq!(classify(b""), SeqType::Unlimit);
        assert_eq!(classify(b"ACGT?"), SeqType::Unlimit);
    }
}
