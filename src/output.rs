use std::fs;
use std::io::{self, Write};
use std::path::Path;

use rsomics_common::{Context, Result, RsomicsError};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::Builder;

pub(crate) fn reject_output_alias<'a>(
    output: &Path,
    inputs: impl IntoIterator<Item = &'a Path>,
) -> Result<()> {
    if output == Path::new("-") {
        return Ok(());
    }
    for input in inputs.into_iter().filter(|input| *input != Path::new("-")) {
        if paths_alias(input, output)? {
            return Err(RsomicsError::ConfigError(format!(
                "output {} is also an input path",
                output.display()
            )));
        }
    }
    Ok(())
}

fn paths_alias(left: &Path, right: &Path) -> Result<bool> {
    if left == right {
        return Ok(true);
    }
    match same_file::is_same_file(left, right) {
        Ok(true) => return Ok(true),
        Ok(false) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(RsomicsError::Io(io::Error::new(
                error.kind(),
                format!(
                    "comparing input {} with output {}: {error}",
                    left.display(),
                    right.display()
                ),
            )));
        }
    }

    let left = canonicalize_if_exists(left, "input")?;
    let right = canonicalize_if_exists(right, "output")?;
    Ok(matches!((left, right), (Some(left), Some(right)) if left == right))
}

fn canonicalize_if_exists(path: &Path, role: &str) -> Result<Option<std::path::PathBuf>> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RsomicsError::Io(io::Error::new(
            error.kind(),
            format!("canonicalizing {role} {}: {error}", path.display()),
        ))),
    }
}

pub(crate) fn with_output<T>(
    path: &Path,
    operation: impl FnOnce(&mut dyn Write) -> Result<T>,
) -> Result<T> {
    if path == Path::new("-") {
        return operation(&mut io::stdout().lock());
    }
    reject_unsupported_compression_suffix(path)?;

    let existing_permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(RsomicsError::Io(io::Error::new(
                error.kind(),
                format!(
                    "reading existing output metadata {}: {error}",
                    path.display()
                ),
            )));
        }
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut builder = Builder::new();
    builder.prefix(".rsomics-seq-");
    #[cfg(unix)]
    if existing_permissions.is_none() {
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    if let Some(permissions) = existing_permissions.as_ref() {
        builder.permissions(permissions.clone());
    }
    let mut temporary = builder.tempfile_in(parent).rs_with_context(|| {
        format!(
            "creating temporary output beside destination {}",
            path.display()
        )
    })?;
    if let Some(permissions) = existing_permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .rs_with_context(|| format!("preserving permissions for output {}", path.display()))?;
    }

    let result = operation(temporary.as_file_mut())?;
    temporary
        .as_file_mut()
        .flush()
        .rs_context("flushing temporary sequence output")?;
    temporary
        .as_file_mut()
        .sync_all()
        .rs_context("syncing temporary sequence output")?;
    temporary.persist(path).map_err(|error| {
        let kind = error.error.kind();
        RsomicsError::Io(io::Error::new(
            kind,
            format!(
                "atomically persisting output {}: {}",
                path.display(),
                error.error
            ),
        ))
    })?;
    Ok(result)
}

fn reject_unsupported_compression_suffix(path: &Path) -> Result<()> {
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
