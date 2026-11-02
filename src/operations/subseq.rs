use std::io::Write;
use std::ops::Range;
use std::str::FromStr;

use rsomics_common::{Result, RsomicsError};
use rsomics_seqio::{Format, Record};
use serde::Serialize;

use crate::input::rewrite_records;

/// Validated signed, nonzero, 1-based inclusive sequence coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Region {
    start: i64,
    end: i64,
}

impl FromStr for Region {
    type Err = RsomicsError;

    /// Parses `START:END`; negative coordinates count from the sequence end.
    ///
    /// # Errors
    ///
    /// Rejects malformed, zero, out-of-range, or negative-start/positive-end
    /// coordinates.
    fn from_str(value: &str) -> Result<Self> {
        let invalid = || {
            RsomicsError::InvalidInput(format!(
                "invalid region `{value}`: expected nonzero START:END coordinates"
            ))
        };
        let (start, end) = value.split_once(':').ok_or_else(invalid)?;
        let parse = |coordinate: &str| {
            let digits = coordinate.strip_prefix('-').unwrap_or(coordinate);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            coordinate.parse::<i64>().map_err(|_| invalid())
        };
        let start = parse(start)?;
        let end = parse(end)?;
        if start == 0 || end == 0 {
            return Err(invalid());
        }
        if start < 0 && end > 0 {
            return Err(RsomicsError::InvalidInput(
                "when start < 0, end should not > 0".into(),
            ));
        }
        Ok(Self { start, end })
    }
}

impl Region {
    fn resolve(self, length: usize) -> Range<usize> {
        let length = length as i128;
        let start = i128::from(self.start);
        let end = i128::from(self.end);
        let start = if start > 0 {
            start - 1
        } else {
            (length + start).max(0)
        };
        let end = if end > 0 {
            end.min(length)
        } else {
            length + end + 1
        };
        if end < 0 || start >= end {
            return 0..0;
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let range = start.min(length) as usize..end.min(length) as usize;
        range
    }

    fn suffix(self) -> Vec<u8> {
        format!(":{}-{} ", self.start, self.end).into_bytes()
    }
}

/// Summary of one record-preserving regional extraction pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubseqReport {
    /// Input path as supplied by the caller, or `-` for stdin.
    pub input: String,
    /// Detected FASTA or FASTQ format.
    pub format: &'static str,
    /// Requested signed, inclusive coordinates before clamping.
    pub region: Region,
    /// Whether headers include the requested coordinate suffix.
    pub region_coord: bool,
    /// Number of records emitted, including empty windows.
    pub records: u64,
}

/// Extracts a region from every record, preserving format and matching qualities.
///
/// Headers are preserved as bytes unless `region_coord` requests SeqKit's
/// coordinate-header formatting. Output is unwrapped, including empty windows.
///
/// # Errors
///
/// Returns an error for malformed input, record count overflow, or output failure.
pub fn extract_subsequences(
    input: &str,
    region: Region,
    region_coord: bool,
    output: &mut dyn Write,
) -> Result<SubseqReport> {
    let suffix = region.suffix();
    let mut header = Vec::new();
    let (format, records) = rewrite_records(input, output, |record, writer| {
        let range = region.resolve(record.seq.len());
        let id = if region_coord {
            coordinate_header(record.id, &suffix, &mut header);
            header.as_slice()
        } else {
            record.id
        };
        writer.write_record(Record {
            id,
            seq: &record.seq[range.clone()],
            qual: record.qual.map(|quality| &quality[range]),
        })
    })?;
    Ok(SubseqReport {
        input: input.to_owned(),
        format: match format {
            Format::Fasta => "FASTA",
            Format::Fastq => "FASTQ",
        },
        region,
        region_coord,
        records,
    })
}

fn coordinate_header(original: &[u8], suffix: &[u8], output: &mut Vec<u8>) {
    let split = original
        .iter()
        .position(|byte| matches!(byte, b' ' | b'\t'));
    let (id, description) = if let Some(split) = split {
        let mut cursor = split + 1;
        // SeqKit's default ID parser advances twice on each separator here.
        while cursor < original.len() && matches!(original[cursor], b' ' | b'\t') {
            cursor += 2;
        }
        (&original[..split], &original[cursor.min(original.len())..])
    } else {
        (original, &[][..])
    };
    output.clear();
    output.extend_from_slice(id);
    output.extend_from_slice(suffix);
    output.extend_from_slice(description);
}
