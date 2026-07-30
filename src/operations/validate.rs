use std::io::Write;

use rsomics_common::{Result, RsomicsError};
use rsomics_seqio::Format;
use serde::Serialize;

use crate::input::scan_records;

/// Successful result of a strict complete-stream validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected input format.
    pub format: &'static str,
    /// Number of records scanned.
    pub records: u64,
    /// Stable validity marker. Parser errors return an error instead.
    pub valid: bool,
}

/// Strictly parses every FASTA/FASTQ record in one stream.
///
/// # Errors
///
/// Returns the first format, decompression, I/O, or count-overflow error.
pub fn validate_sequences(input: &str) -> Result<ValidationReport> {
    let mut records = 0u64;
    let format = scan_records(input, |_| {
        records = records.checked_add(1).ok_or_else(|| {
            RsomicsError::InvalidInput("record count exceeds u64 capacity".into())
        })?;
        Ok(())
    })?;
    Ok(ValidationReport {
        input: input.to_owned(),
        format: match format {
            Format::Fasta => "FASTA",
            Format::Fastq => "FASTQ",
        },
        records,
        valid: true,
    })
}

/// Writes the stable validation TSV report.
///
/// # Errors
///
/// Returns an I/O error if the report cannot be written or flushed.
pub fn write_validation_tsv(report: &ValidationReport, output: &mut dyn Write) -> Result<()> {
    writeln!(output, "input\tformat\trecords\tvalid").map_err(RsomicsError::Io)?;
    writeln!(
        output,
        "{}\t{}\t{}\t{}",
        report.input, report.format, report.records, report.valid
    )
    .map_err(RsomicsError::Io)?;
    output.flush().map_err(RsomicsError::Io)
}
