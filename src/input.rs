use std::io::BufRead;
use std::path::Path;

use rsomics_common::{Context, Result};
use rsomics_seqio::{Format, PathReader, Reader, Record, Writer, open_path, open_reader};

pub(crate) trait RecordReader {
    fn format(&self) -> Format;
    fn read_record(&mut self) -> Result<Option<Record<'_>>>;
}

impl<R: BufRead> RecordReader for Reader<R> {
    fn format(&self) -> Format {
        self.format()
    }

    fn read_record(&mut self) -> Result<Option<Record<'_>>> {
        self.read_record()
    }
}

impl RecordReader for PathReader {
    fn format(&self) -> Format {
        self.format()
    }

    fn read_record(&mut self) -> Result<Option<Record<'_>>> {
        self.read_record()
    }
}

pub(crate) fn scan_records<F>(input: &str, mut visit: F) -> Result<Format>
where
    F: for<'record> FnMut(Record<'record>) -> Result<()>,
{
    scan_records_with_format(input, |_, record| visit(record))
}

pub(crate) fn scan_records_with_format<F>(input: &str, mut visit: F) -> Result<Format>
where
    F: for<'record> FnMut(Format, Record<'record>) -> Result<()>,
{
    if input == "-" {
        let stdin = std::io::stdin();
        let reader = open_reader(stdin.lock()).rs_context("opening stdin")?;
        drain(input, reader, &mut visit)
    } else {
        let reader = open_path(Path::new(input))
            .rs_with_context(|| format!("opening sequence input {input}"))?;
        drain(input, reader, &mut visit)
    }
}

pub(crate) fn copy_matching_records<F>(
    input: &str,
    output: &mut dyn std::io::Write,
    mut keep: F,
) -> Result<(Format, u64, u64)>
where
    F: for<'record> FnMut(Record<'record>) -> Result<bool>,
{
    if input == "-" {
        let stdin = std::io::stdin();
        let reader = open_reader(stdin.lock()).rs_context("opening stdin")?;
        drain_matching(input, reader, output, &mut keep)
    } else {
        let reader = open_path(Path::new(input))
            .rs_with_context(|| format!("opening sequence input {input}"))?;
        drain_matching(input, reader, output, &mut keep)
    }
}

fn drain<R, F>(input: &str, mut reader: R, visit: &mut F) -> Result<Format>
where
    R: RecordReader,
    F: for<'record> FnMut(Format, Record<'record>) -> Result<()>,
{
    let format = reader.format();
    let mut record_number = 0u64;
    while let Some(record) = reader
        .read_record()
        .rs_with_context(|| format!("reading {input} after record {record_number}"))?
    {
        record_number += 1;
        visit(format, record)
            .rs_with_context(|| format!("processing {input} record {record_number}"))?;
    }
    Ok(format)
}

fn drain_matching<R, F>(
    input: &str,
    mut reader: R,
    output: &mut dyn std::io::Write,
    keep: &mut F,
) -> Result<(Format, u64, u64)>
where
    R: RecordReader,
    F: for<'record> FnMut(Record<'record>) -> Result<bool>,
{
    let format = reader.format();
    let mut writer = Writer::new(output, format);
    let mut total_records = 0u64;
    let mut matched_records = 0u64;
    while let Some(record) = reader
        .read_record()
        .rs_with_context(|| format!("reading {input} after record {total_records}"))?
    {
        total_records = total_records.checked_add(1).ok_or_else(|| {
            rsomics_common::RsomicsError::InvalidInput("record count exceeds u64 capacity".into())
        })?;
        if keep(record).rs_with_context(|| format!("processing {input} record {total_records}"))? {
            writer.write_record(record)?;
            matched_records = matched_records.checked_add(1).ok_or_else(|| {
                rsomics_common::RsomicsError::InvalidInput(
                    "matched record count exceeds u64 capacity".into(),
                )
            })?;
        }
    }
    writer.finish()?;
    Ok((format, total_records, matched_records))
}
