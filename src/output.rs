use std::io::Write;
use std::path::Path;

use rsomics_common::{Result, RsomicsError, write_output};

pub(crate) use rsomics_common::reject_output_alias;

pub(crate) fn with_output<T>(
    path: &Path,
    operation: impl FnOnce(&mut dyn Write) -> Result<T>,
) -> Result<T> {
    reject_unsupported_compression_suffix(path)?;
    write_output(Some(path), operation)
}

fn reject_unsupported_compression_suffix(path: &Path) -> Result<()> {
    if path == Path::new("-") {
        return Ok(());
    }
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("gz" | "bgz" | "bgzf")) {
        return Err(RsomicsError::ConfigError(format!(
            "compressed output is not implemented; refusing to write uncompressed data to {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn operation_failure_does_not_replace_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.tsv");
        fs::write(&output, b"existing output\n").unwrap();

        let error = with_output(&output, |temporary| {
            temporary.write_all(b"partial replacement\n")?;
            Err::<(), _>(RsomicsError::InvalidInput("late failure".into()))
        })
        .unwrap_err();

        assert!(error.to_string().contains("late failure"));
        assert_eq!(fs::read(output).unwrap(), b"existing output\n");
    }

    #[test]
    fn absent_output_is_not_an_alias() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.fa");
        let output = directory.path().join("output.tsv");
        fs::write(&input, b">record\nACGT\n").unwrap();

        reject_output_alias(&output, [input.as_path()]).unwrap();
    }

    #[test]
    fn compressed_suffix_is_rejected_before_file_creation() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("records.fa.gz");
        let error = with_output(&output, |writer| {
            writer.write_all(b">one\nACGT\n")?;
            Ok(())
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("compressed output is not implemented")
        );
        assert!(!output.exists());
    }
}
