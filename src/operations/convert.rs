use std::io::Write;

use clap::ValueEnum;
use rsomics_common::{Result, RsomicsError};
use rsomics_seqio::{Format, Record, Writer};
use serde::Serialize;

use crate::input::scan_records;

/// Output format selected for [`convert_sequences`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ConvertFormat {
    /// FASTA without quality scores.
    Fasta,
    /// FASTQ with required quality scores.
    Fastq,
}

impl ConvertFormat {
    fn seqio(self) -> Format {
        match self {
            Self::Fasta => Format::Fasta,
            Self::Fastq => Format::Fastq,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Fasta => "FASTA",
            Self::Fastq => "FASTQ",
        }
    }
}

/// Summary of one format-normalization pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvertReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected source format.
    pub input_format: &'static str,
    /// Requested output format.
    pub output_format: &'static str,
    /// Number of records converted.
    pub records: u64,
}

/// Rewrites one FASTA/FASTQ stream in the selected format.
///
/// FASTQ-to-FASTA conversion drops qualities. FASTA-to-FASTQ conversion is
/// rejected because this operation never invents quality scores.
///
/// # Errors
///
/// Returns an error for malformed input, count overflow, output failure, or a
/// request to write FASTQ when the input records have no quality scores.
pub fn convert_sequences(
    input: &str,
    target: ConvertFormat,
    output: &mut dyn Write,
) -> Result<ConvertReport> {
    let target_format = target.seqio();
    let mut writer = Writer::new(output, target_format);
    let mut records = 0u64;
    let input_format = scan_records(input, |record| {
        if target_format == Format::Fastq && record.qual.is_none() {
            return Err(RsomicsError::InvalidInput(
                "cannot convert FASTA to FASTQ without quality scores".into(),
            ));
        }
        let converted = Record {
            id: record.id,
            seq: record.seq,
            qual: if target_format == Format::Fastq {
                record.qual
            } else {
                None
            },
        };
        writer.write_record(converted)?;
        records = records.checked_add(1).ok_or_else(|| {
            RsomicsError::InvalidInput("record count exceeds u64 capacity".into())
        })?;
        Ok(())
    })?;
    writer.finish()?;

    Ok(ConvertReport {
        input: input.to_owned(),
        input_format: format_name(input_format),
        output_format: target.as_str(),
        records,
    })
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
    fn fasta_to_fastq_never_invents_quality() {
        let input = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(input.path(), b">one\nACGT\n").unwrap();
        let mut output = Vec::new();
        let error = convert_sequences(
            input.path().to_str().unwrap(),
            ConvertFormat::Fastq,
            &mut output,
        )
        .unwrap_err();
        assert!(error.to_string().contains("without quality"));
        assert!(output.is_empty());
    }
}
