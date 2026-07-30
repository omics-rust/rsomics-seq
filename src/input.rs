use std::io::BufRead;
use std::path::Path;

use rsomics_common::{Context, Result};
use rsomics_seqio::{Format, PathReader, Reader, Record, open_path, open_reader};

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

fn drain<R, F>(input: &str, mut reader: R, visit: &mut F) -> Result<Format>
where
    R: RecordReader,
    F: for<'record> FnMut(Record<'record>) -> Result<()>,
{
    let format = reader.format();
    let mut record_number = 0u64;
    while let Some(record) = reader
        .read_record()
        .rs_with_context(|| format!("reading {input} after record {record_number}"))?
    {
        record_number += 1;
        visit(record).rs_with_context(|| format!("processing {input} record {record_number}"))?;
    }
    Ok(format)
}
