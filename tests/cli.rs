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
        _ => panic!("unknown operation"),
    }
    command.output().unwrap()
}

#[test]
fn help_succeeds_without_input() {
    for args in [&["--help"][..], &["stats", "--help"], &["kmers", "--help"]] {
        let output = run(args);
        assert!(
            output.status.success(),
            "args={args:?}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
fn exact_normalized_and_hardlink_output_aliases_preserve_inputs() {
    for operation in ["stats", "kmers"] {
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

    for operation in ["stats", "kmers"] {
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

    for operation in ["stats", "kmers"] {
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
