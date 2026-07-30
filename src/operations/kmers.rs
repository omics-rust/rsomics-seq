use std::io::Write;

use rsomics_common::{Result, RsomicsError};
use rsomics_kmer::{KmerCounts, decode};
use rsomics_seqio::Format;
use serde::Serialize;

use crate::input::scan_records;

/// Options controlling exact DNA k-mer counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmerOptions {
    /// K-mer length in `1..=32`.
    pub k: usize,
    /// Collapse a k-mer and its reverse complement.
    pub canonical: bool,
    /// Emit only k-mers with at least this count.
    pub min_count: u64,
}

/// One deterministic k-mer output row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KmerRow {
    /// Uppercase A/C/G/T k-mer.
    pub kmer: String,
    /// Number of observed windows.
    pub count: u64,
}

/// Result of the `kmers` operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KmerReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected input format.
    pub format: &'static str,
    /// K-mer length.
    pub k: usize,
    /// Whether reverse complements were collapsed.
    pub canonical: bool,
    /// Minimum count required for an emitted row.
    pub min_count: u64,
    /// Complete fixed-width windows before ambiguity filtering.
    pub candidate_windows: u64,
    /// Windows containing only A/C/G/T.
    pub valid_windows: u64,
    /// Windows skipped because at least one byte was not A/C/G/T.
    pub skipped_windows: u64,
    /// Number of distinct valid k-mers before `min_count` filtering.
    pub distinct_kmers: u64,
    /// Rows retained after `min_count` filtering.
    pub emitted_kmers: u64,
    /// Deterministically sorted counts.
    pub counts: Vec<KmerRow>,
}

/// Counts exact DNA k-mers across one FASTA or FASTQ stream.
///
/// Windows containing a non-ACGT byte are skipped. Rows are ordered by
/// descending count and then lexicographically by k-mer.
///
/// # Errors
///
/// Returns an error for invalid options, unreadable or malformed input, or
/// counts that cannot be represented by the report types.
pub fn count_kmers(input: &str, options: KmerOptions) -> Result<KmerReport> {
    if options.min_count == 0 {
        return Err(RsomicsError::InvalidInput(
            "minimum count must be at least 1".into(),
        ));
    }

    let mut accumulator = KmerCounts::try_new(options.k, options.canonical)
        .map_err(|error| RsomicsError::InvalidInput(error.to_string()))?;

    let mut candidate_windows = 0u64;
    let format = scan_records(input, |record| {
        let windows = if record.seq.len() >= options.k {
            record.seq.len() - options.k + 1
        } else {
            0
        };
        candidate_windows = candidate_windows
            .checked_add(u64::try_from(windows).map_err(|_| {
                RsomicsError::InvalidInput("k-mer window count cannot fit in u64".into())
            })?)
            .ok_or_else(|| {
                RsomicsError::InvalidInput("k-mer window count exceeds u64 capacity".into())
            })?;
        accumulator
            .count_seq(record.seq)
            .map_err(|error| RsomicsError::InvalidInput(error.to_string()))
    })?;

    let valid_windows = accumulator.total();
    let skipped_windows = candidate_windows
        .checked_sub(valid_windows)
        .ok_or_else(|| {
            RsomicsError::InvalidInput("valid k-mer count exceeds candidate windows".into())
        })?;
    let distinct_kmers = u64::try_from(accumulator.len())
        .map_err(|_| RsomicsError::InvalidInput("distinct count cannot fit in u64".into()))?;

    let mut encoded: Vec<_> = accumulator
        .counts
        .into_iter()
        .filter(|(_, count)| *count >= options.min_count)
        .collect();
    encoded.sort_unstable_by(|(left_kmer, left_count), (right_kmer, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_kmer.cmp(right_kmer))
    });

    let mut counts = Vec::with_capacity(encoded.len());
    for (encoded_kmer, count) in encoded {
        let bytes = decode(encoded_kmer, options.k);
        let kmer = String::from_utf8(bytes)
            .map_err(|error| RsomicsError::InvalidInput(error.to_string()))?;
        counts.push(KmerRow { kmer, count });
    }
    let emitted_kmers = u64::try_from(counts.len())
        .map_err(|_| RsomicsError::InvalidInput("emitted count cannot fit in u64".into()))?;

    Ok(KmerReport {
        input: input.to_owned(),
        format: match format {
            Format::Fasta => "FASTA",
            Format::Fastq => "FASTQ",
        },
        k: options.k,
        canonical: options.canonical,
        min_count: options.min_count,
        candidate_windows,
        valid_windows,
        skipped_windows,
        distinct_kmers,
        emitted_kmers,
        counts,
    })
}

/// Writes a stable two-column TSV count table.
///
/// # Errors
///
/// Returns an error when the output cannot be written or flushed.
pub fn write_kmer_tsv(report: &KmerReport, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "kmer\tcount").map_err(RsomicsError::Io)?;
    for row in &report.counts {
        writeln!(output, "{}\t{}", row.kmer, row.count).map_err(RsomicsError::Io)?;
    }
    output.flush().map_err(RsomicsError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsomics_kmer::KmerError;

    #[test]
    fn foundation_constructor_rejects_invalid_user_k() {
        for k in [0, 33, usize::MAX] {
            let error = KmerCounts::try_new(k, false).unwrap_err();
            assert!(matches!(error, KmerError::KOutOfRange(actual) if actual == k));
            assert_eq!(error.to_string(), format!("k must be in 1..=32 (got {k})"));
        }
    }

    #[test]
    fn fallible_constructor_accepts_codec_boundaries() {
        assert!(KmerCounts::try_new(1, false).is_ok());
        assert!(KmerCounts::try_new(32, true).is_ok());
    }
}
