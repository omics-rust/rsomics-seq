use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"))
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .args(args)
        .current_dir(root())
        .output()
        .expect("run rsomics-seq")
}

fn error_envelope(output: &std::process::Output) -> serde_json::Value {
    let first_line = output.stderr.split(|&byte| byte == b'\n').next().unwrap();
    serde_json::from_slice(first_line).unwrap()
}

fn run_with_output(operation: &str, input: &Path, output: &Path) -> std::process::Output {
    let mut command = Command::new(binary());
    match operation {
        "stats" => {
            command.arg("stats").arg(input).arg("--output").arg(output);
        }
        "kmers" => {
            command
                .args(["kmers", "-k", "3"])
                .arg(input)
                .arg("--output")
                .arg(output);
        }
        "grep" => {
            command
                .args(["grep", "--pattern", "seq1"])
                .arg(input)
                .arg("--output")
                .arg(output);
        }
        "convert" => {
            command
                .args(["convert", "--to", "fasta"])
                .arg(input)
                .arg("--output")
                .arg(output);
        }
        "validate" => {
            command
                .arg("validate")
                .arg(input)
                .arg("--output")
                .arg(output);
        }
        _ => panic!("unknown operation"),
    }
    command.output().unwrap()
}

#[test]
fn help_succeeds_without_input() {
    for args in [
        &["--help"][..],
        &["stats", "--help"],
        &["kmers", "--help"],
        &["grep", "--help"],
        &["convert", "--help"],
        &["validate", "--help"],
    ] {
        let output = run(args);
        assert!(
            output.status.success(),
            "args={args:?}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn help_exposes_only_applicable_global_options() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Global options:"));
    assert!(help.contains("--json"));
    for absent in ["--threads", "--seed", "--quiet", "--verbose"] {
        assert!(!help.contains(absent), "{absent} should not be advertised");
    }

    let nested = run(&["help", "kmers"]);
    assert!(nested.status.success());
    assert!(
        String::from_utf8(nested.stdout)
            .unwrap()
            .contains("Usage: rsomics-seq kmers")
    );
}

#[test]
fn grep_literal_modes_preserve_format_and_order() {
    let id = run(&["grep", "--pattern", "seq1", "tests/golden/records.fa"]);
    assert!(id.status.success());
    assert_eq!(id.stdout, b">seq1 alpha\nACGTACGT\n");

    let name = run(&[
        "grep",
        "--by-name",
        "--pattern",
        "seq2 beta",
        "tests/golden/records.fa",
    ]);
    assert!(name.status.success());
    assert_eq!(name.stdout, b">seq2 beta\nTTTTAAAACCCCGGGG\n");

    let insensitive_inverted = run(&[
        "grep",
        "--ignore-case",
        "--invert-match",
        "--pattern",
        "SEQ3",
        "tests/golden/records.fa",
    ]);
    assert!(insensitive_inverted.status.success());
    assert_eq!(
        insensitive_inverted.stdout,
        b">seq1 alpha\nACGTACGT\n>seq2 beta\nTTTTAAAACCCCGGGG\n"
    );
}

#[test]
fn grep_sequence_searches_both_strands_unless_positive_only() {
    let both = run(&[
        "grep",
        "--by-seq",
        "--pattern",
        "GGGGTTTT",
        "tests/golden/records.fa",
    ]);
    assert!(both.status.success());
    assert_eq!(both.stdout, b">seq2 beta\nTTTTAAAACCCCGGGG\n");

    let positive = run(&[
        "grep",
        "--by-seq",
        "--only-positive-strand",
        "--pattern",
        "GGGGTTTT",
        "tests/golden/records.fa",
    ]);
    assert!(positive.status.success());
    assert!(positive.stdout.is_empty());
}

#[test]
fn grep_and_convert_fastq_preserve_records() {
    let grep = run(&["grep", "--pattern", "r2", "tests/golden/stats.fq"]);
    assert!(grep.status.success());
    assert_eq!(grep.stdout, b"@r2\nACGTNN\n+\nIIIIII\n");

    let convert = run(&["convert", "--to", "fasta", "tests/golden/stats.fq"]);
    assert!(convert.status.success());
    assert_eq!(convert.stdout, b">r1\nACGT\n>r2\nACGTNN\n>r3\nGGCCGGCC\n");
}

#[test]
fn convert_to_fastq_requires_real_quality_scores() {
    let result = run(&["convert", "--to", "fastq", "tests/golden/stats.fa"]);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("without quality scores"));
}

#[test]
fn validate_reports_complete_strict_scan() {
    let text = run(&["validate", "tests/golden/stats.fq"]);
    assert!(text.status.success());
    assert_eq!(
        text.stdout,
        b"input\tformat\trecords\tvalid\ntests/golden/stats.fq\tFASTQ\t3\ttrue\n"
    );

    let json = run(&["--json", "validate", "tests/golden/stats.fa"]);
    assert!(json.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(envelope["result"]["operation"], "validate");
    assert_eq!(envelope["result"]["data"]["records"], 5);
    assert_eq!(envelope["result"]["data"]["valid"], true);
}

#[test]
fn validate_rejects_damage_after_valid_prefix_without_replacing_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("trailing-damage.fq");
    let output = directory.path().join("validation.tsv");
    fs::write(
        &input,
        b"@one\nACGT\n+\nIIII\n@two\nTGCA\n+\nFFFF\n@broken\nACGT\n+\nIII\n",
    )
    .unwrap();
    fs::write(&output, b"existing report\n").unwrap();

    let result = run(&[
        "validate",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);

    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("truncated FASTQ quality"));
    assert_eq!(fs::read(output).unwrap(), b"existing report\n");
}

#[test]
fn stats_fasta_and_fastq_match_frozen_seqkit_goldens() {
    for (input, expected) in [
        ("tests/golden/stats.fa", "tests/golden/stats.fa.seqkit.tsv"),
        ("tests/golden/stats.fq", "tests/golden/stats.fq.seqkit.tsv"),
    ] {
        let output = run(&["stats", input]);
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, std::fs::read(root().join(expected)).unwrap());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn stats_reads_stdin() {
    let fixture = std::fs::read(root().join("tests/golden/stats.fa")).unwrap();
    let mut child = Command::new(binary())
        .args(["stats", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&fixture).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\n-\tFASTA\tDNA\t5\t84\t4\t16.8\t32\n"));
}

#[test]
fn json_uses_common_envelope_and_does_not_mix_tsv() {
    let output = run(&["--json", "stats", "tests/golden/stats.fa"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], "1.0");
    assert_eq!(value["tool"], "rsomics-seq");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["result"]["operation"], "stats");
    assert_eq!(value["result"]["data"]["rows"][0]["num_seqs"], 5);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("file\tformat"));
}

#[test]
fn kmers_match_frozen_tables() {
    for (extra, expected) in [
        (&[][..], "tests/golden/kmers.k3.tsv"),
        (&["--canonical"][..], "tests/golden/kmers.k3.canonical.tsv"),
    ] {
        let mut args = vec!["kmers", "-k", "3"];
        args.extend_from_slice(extra);
        args.push("tests/golden/kmers.fa");
        let output = run(&args);
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, std::fs::read(root().join(expected)).unwrap());
    }
}

#[test]
fn kmer_codec_boundaries_are_accepted() {
    for k in ["1", "32"] {
        let output = run(&["kmers", "-k", k, "tests/golden/kmers.fa"]);
        assert!(
            output.status.success(),
            "k={k}, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.starts_with(b"kmer\tcount\n"));
    }
}

#[test]
fn kmer_json_reports_ambiguity_accounting() {
    let output = run(&["--json", "kmers", "-k", "3", "tests/golden/kmers.fa"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &value["result"]["data"];
    assert_eq!(report["candidate_windows"], 13);
    assert_eq!(report["valid_windows"], 10);
    assert_eq!(report["skipped_windows"], 3);
    assert_eq!(report["distinct_kmers"], 4);
    assert_eq!(report["min_count"], 1);
}

#[test]
fn invalid_k_is_stable_invalid_input_without_panicking() {
    for k in ["0", "33", "18446744073709551615"] {
        let invalid = run(&["kmers", "-k", k, "tests/golden/kmers.fa"]);
        assert_eq!(invalid.status.code(), Some(1), "k={k}");
        assert!(invalid.stdout.is_empty(), "k={k}");
        assert!(
            String::from_utf8_lossy(&invalid.stderr)
                .contains(&format!("k must be in 1..=32 (got {k})")),
            "k={k}, stderr={}",
            String::from_utf8_lossy(&invalid.stderr)
        );
    }

    let invalid_min_count = run(&[
        "kmers",
        "-k",
        "3",
        "--min-count",
        "0",
        "tests/golden/kmers.fa",
    ]);
    assert_eq!(invalid_min_count.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&invalid_min_count.stderr).contains("at least 1"));

    let json = run(&["--json", "kmers", "-k", "33", "tests/golden/kmers.fa"]);
    assert_eq!(json.status.code(), Some(1));
    assert!(json.stdout.is_empty());
    let envelope = error_envelope(&json);
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["error"]["kind"], "InvalidInput");
    assert_eq!(envelope["exit_code"], 1);
}

#[test]
fn json_output_conflict_is_config_error_exit_two() {
    let directory = tempfile::tempdir().unwrap();
    let output_path = directory.path().join("ignored.tsv");
    let conflict = run(&[
        "--json",
        "stats",
        "--output",
        output_path.to_str().unwrap(),
        "tests/golden/stats.fa",
    ]);
    assert_eq!(conflict.status.code(), Some(2));
    assert!(conflict.stdout.is_empty());
    let envelope = error_envelope(&conflict);
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["error"]["kind"], "ConfigError");
    assert_eq!(envelope["exit_code"], 2);
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("--output"));
    assert!(!output_path.exists());
}

#[test]
fn compressed_output_suffix_is_rejected_instead_of_writing_plain_text() {
    let directory = tempfile::tempdir().unwrap();
    let output_path = directory.path().join("records.fa.gz");
    let result = run(&[
        "convert",
        "--to",
        "fasta",
        "--output",
        output_path.to_str().unwrap(),
        "tests/golden/stats.fa",
    ]);

    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("compressed output is not implemented")
    );
    assert!(!output_path.exists());
}

#[test]
fn exact_normalized_and_hardlink_output_aliases_preserve_inputs() {
    for operation in ["stats", "kmers", "grep", "convert", "validate"] {
        for alias in ["exact", "normalized", "hardlink"] {
            let directory = tempfile::tempdir().unwrap();
            let input = directory.path().join("input.fa");
            let original = fs::read(root().join("tests/golden/kmers.fa")).unwrap();
            fs::write(&input, &original).unwrap();
            let output = match alias {
                "exact" => input.clone(),
                "normalized" => directory.path().join(".").join("input.fa"),
                "hardlink" => {
                    let output = directory.path().join("output.fa");
                    fs::hard_link(&input, &output).unwrap();
                    output
                }
                _ => unreachable!(),
            };

            let result = run_with_output(operation, &input, &output);
            assert_eq!(
                result.status.code(),
                Some(2),
                "operation={operation}, alias={alias}, stderr={}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(fs::read(input).unwrap(), original);
        }
    }
}

#[cfg(unix)]
#[test]
fn symlink_output_aliases_preserve_inputs() {
    use std::os::unix::fs::symlink;

    for operation in ["stats", "kmers", "grep", "convert", "validate"] {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.fa");
        let output = directory.path().join("output.tsv");
        let original = fs::read(root().join("tests/golden/kmers.fa")).unwrap();
        fs::write(&input, &original).unwrap();
        symlink(&input, &output).unwrap();

        let result = run_with_output(operation, &input, &output);
        assert_eq!(
            result.status.code(),
            Some(2),
            "operation={operation}, stderr={}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(input).unwrap(), original);
    }
}

#[test]
fn named_outputs_are_transactional_on_operation_failure() {
    let directory = tempfile::tempdir().unwrap();
    let malformed = directory.path().join("malformed.fq");
    let output = directory.path().join("output.tsv");
    fs::write(&malformed, b"@read\nACGT\n+\n!!!\n").unwrap();

    for operation in ["stats", "kmers", "grep", "convert", "validate"] {
        fs::write(&output, b"existing output\n").unwrap();
        let result = run_with_output(operation, &malformed, &output);
        assert_eq!(
            result.status.code(),
            Some(1),
            "operation={operation}, stderr={}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(&output).unwrap(), b"existing output\n");
    }
}

#[cfg(unix)]
#[test]
fn named_output_permissions_follow_umask_or_preserve_existing_mode() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let input = root().join("tests/golden/stats.fa");
    let control = directory.path().join("control");
    let new_output = directory.path().join("new.tsv");
    fs::write(&control, b"control").unwrap();
    let created = run_with_output("stats", &input, &new_output);
    assert!(created.status.success());
    assert_eq!(
        fs::metadata(&new_output).unwrap().permissions().mode() & 0o777,
        fs::metadata(&control).unwrap().permissions().mode() & 0o777
    );

    fs::set_permissions(&new_output, fs::Permissions::from_mode(0o666)).unwrap();
    let replaced = run_with_output("stats", &input, &new_output);
    assert!(replaced.status.success());
    assert_eq!(
        fs::metadata(new_output).unwrap().permissions().mode() & 0o777,
        0o666
    );
}

#[test]
fn missing_output_parent_uses_common_io_exit_code() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("missing").join("output.tsv");
    let result = run_with_output("stats", &root().join("tests/golden/stats.fa"), &output);
    assert_eq!(result.status.code(), Some(4));
    assert!(result.stdout.is_empty());
}

#[test]
fn empty_and_malformed_inputs_fail_nonzero() {
    let empty = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
    let empty_output = Command::new(binary())
        .args(["stats", empty.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!empty_output.status.success());
    assert!(empty_output.stdout.is_empty());

    let mut malformed = tempfile::Builder::new().suffix(".fq").tempfile().unwrap();
    malformed.write_all(b"@read\nACGT\n+\n!!!\n").unwrap();
    malformed.flush().unwrap();
    let malformed_output = Command::new(binary())
        .args(["stats", malformed.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!malformed_output.status.success());
    assert!(malformed_output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&malformed_output.stderr).contains("reading"));
}

#[test]
fn stats_multiple_inputs_preserve_order_and_reject_mixed_stdin() {
    let output = run(&[
        "--json",
        "stats",
        "tests/golden/stats.fq",
        "tests/golden/stats.fa",
    ]);
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        envelope["result"]["data"]["rows"][0]["file"],
        "tests/golden/stats.fq"
    );
    assert_eq!(
        envelope["result"]["data"]["rows"][1]["file"],
        "tests/golden/stats.fa"
    );

    let mixed = run(&["stats", "-", "tests/golden/stats.fa"]);
    assert_eq!(mixed.status.code(), Some(1));
    assert!(mixed.stdout.is_empty());
    assert!(String::from_utf8_lossy(&mixed.stderr).contains("cannot be combined"));
}

#[test]
fn stats_reads_content_detected_gzip() {
    let file = tempfile::Builder::new().suffix(".data").tempfile().unwrap();
    let mut writer = rsomics_seqio::create_path(
        file.path(),
        rsomics_seqio::Format::Fasta,
        rsomics_seqio::Compression::Gzip { level: 4 },
    )
    .unwrap();
    writer
        .write_record(rsomics_seqio::Record {
            id: b"one",
            seq: b"ACGT",
            qual: None,
        })
        .unwrap();
    writer.finish().unwrap();

    let output = Command::new(binary())
        .args(["stats", file.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("\tFASTA\tDNA\t1\t4\t4\t4.0\t4"));
}

#[test]
fn grep_convert_and_validate_read_content_detected_gzip() {
    let file = tempfile::Builder::new().suffix(".data").tempfile().unwrap();
    let mut writer = rsomics_seqio::create_path(
        file.path(),
        rsomics_seqio::Format::Fastq,
        rsomics_seqio::Compression::Gzip { level: 4 },
    )
    .unwrap();
    writer
        .write_record(rsomics_seqio::Record {
            id: b"one sample",
            seq: b"ACGT",
            qual: Some(b"IIII"),
        })
        .unwrap();
    writer.finish().unwrap();
    let path = file.path().to_str().unwrap();

    let grep = Command::new(binary())
        .args(["grep", "--pattern", "one", path])
        .output()
        .unwrap();
    assert!(grep.status.success());
    assert_eq!(grep.stdout, b"@one sample\nACGT\n+\nIIII\n");

    let convert = Command::new(binary())
        .args(["convert", "--to", "fasta", path])
        .output()
        .unwrap();
    assert!(convert.status.success());
    assert_eq!(convert.stdout, b">one sample\nACGT\n");

    let validate = Command::new(binary())
        .args(["validate", path])
        .output()
        .unwrap();
    assert!(validate.status.success());
    assert!(String::from_utf8_lossy(&validate.stdout).contains("\tFASTQ\t1\ttrue"));
}
