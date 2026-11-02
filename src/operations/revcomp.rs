use std::io::Write;

use rsomics_common::Result;
use rsomics_seqio::{Format, Record};
use serde::Serialize;

use crate::input::rewrite_records;
use crate::operations::stats::{SeqType, classify_record};

/// Summary of a complete reverse-complement pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevcompReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected FASTA or FASTQ format.
    pub format: &'static str,
    /// Alphabet inferred once from the first record.
    pub seq_type: SeqType,
    /// Number of records transformed.
    pub records: u64,
}

/// Reverse-complements nucleotide records and reverses their quality scores.
///
/// One alphabet is inferred from the first record. Protein and unrestricted
/// records are reversed without complementing. Headers and letter case are
/// preserved; sequence and quality scratch capacity is reused across records.
///
/// # Errors
///
/// Returns an error for malformed input, record count overflow, or output failure.
pub fn reverse_complement_sequences(input: &str, output: &mut dyn Write) -> Result<RevcompReport> {
    let mut seq_type = None;
    let mut sequence = Vec::new();
    let mut quality = Vec::new();
    let (format, records) = rewrite_records(input, output, |record, writer| {
        let kind = *seq_type.get_or_insert_with(|| classify_record(record.seq));
        sequence.clear();
        sequence.extend(record.seq.iter().rev().map(|&byte| complement(byte, kind)));
        quality.clear();
        let qual = record.qual.map(|source| {
            quality.extend(source.iter().rev().copied());
            quality.as_slice()
        });
        writer.write_record(Record {
            id: record.id,
            seq: &sequence,
            qual,
        })
    })?;
    Ok(RevcompReport {
        input: input.to_owned(),
        format: match format {
            Format::Fasta => "FASTA",
            Format::Fastq => "FASTQ",
        },
        seq_type: seq_type.unwrap_or(SeqType::Unlimit),
        records,
    })
}

fn complement(byte: u8, kind: SeqType) -> u8 {
    if matches!(kind, SeqType::Protein | SeqType::Unlimit) {
        return byte;
    }
    match byte {
        b'A' if kind == SeqType::Rna => b'U',
        b'a' if kind == SeqType::Rna => b'u',
        b'A' => b'T',
        b'a' => b't',
        b'T' if kind == SeqType::Dna => b'A',
        b't' if kind == SeqType::Dna => b'a',
        b'U' if kind == SeqType::Rna => b'A',
        b'u' if kind == SeqType::Rna => b'a',
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
    }
}
