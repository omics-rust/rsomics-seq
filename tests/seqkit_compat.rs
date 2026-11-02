use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn available() -> bool {
    Command::new("seqkit")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn require_or_skip() -> bool {
    if available() {
        return true;
    }
    assert_ne!(
        std::env::var("RSOMICS_REQUIRE_SEQKIT").as_deref(),
        Ok("1"),
        "SeqKit is required but not on PATH"
    );
    eprintln!("skipping live SeqKit differential; frozen goldens still run");
    false
}

fn run(program: &Path, args: &[&str]) -> std::process::Output {
    Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn run_strings(program: &Path, args: &[String]) -> std::process::Output {
    Command::new(program)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn run_stdin(program: &Path, args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("compatibility executable must be available");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn signed_subsequences_match_complete_live_seqkit_records() {
    assert!(available(), "live subsequence differential requires SeqKit");
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    for input in [
        b">one\t desc  more\nACGTac\n>two\nTG\n".as_slice(),
        b"@one\t desc  more\nACGTac\n+\n!\"#$%&\n@two\nTG\n+\nIJ\n".as_slice(),
    ] {
        for region in [
            "1:3", "-3:-1", "2:-2", "1:100", "-100:-1", "5:2", "20:30", "1:-100",
        ] {
            for coord in [false, true] {
                let mut our_args = vec!["subseq", "-r", region];
                let mut source_args = vec!["subseq", "-w", "0", "-r", region];
                if coord {
                    our_args.push("-R");
                    source_args.push("-R");
                }
                let actual = run_stdin(&ours, &our_args, input);
                let source = run_stdin(Path::new("seqkit"), &source_args, input);
                assert!(
                    actual.status.success(),
                    "{our_args:?}: {}",
                    String::from_utf8_lossy(&actual.stderr)
                );
                assert!(
                    source.status.success(),
                    "{source_args:?}: {}",
                    String::from_utf8_lossy(&source.stderr)
                );
                assert_eq!(
                    actual.stdout, source.stdout,
                    "region={region}, coord={coord}"
                );
            }
        }
    }
}

#[test]
fn reverse_complement_matches_complete_live_seqkit_records() {
    assert!(
        available(),
        "live reverse-complement differential requires SeqKit"
    );
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    for input in [
        b">one\t full header\nACGTacRysWKmbdHVN\n>later\nAaTtUu\n".as_slice(),
        b">one\t full header\nACGUacRysWKmbdHVN\n>later\nAaTtUu\n",
        b">one\nMKWVTF\n>later\nAaTtUu\n",
        b">one\nACGT?\n>later\nAaTtUu\n",
        b">one\n\n>later\nAaTtUu\n",
        b"@one\t full header\nACGTac\n+\n!\"#$%&\n@two\nTG\n+\nIJ\n",
        b"@rna\nACGUac\n+\n!\"#$%&\n",
    ] {
        let actual = run_stdin(&ours, &["revcomp"], input);
        let source = run_stdin(Path::new("seqkit"), &["seq", "-r", "-p", "-w", "0"], input);
        assert!(
            actual.status.success(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert!(
            source.status.success(),
            "{}",
            String::from_utf8_lossy(&source.stderr)
        );
        assert_eq!(
            actual.stdout,
            source.stdout,
            "input={}",
            String::from_utf8_lossy(input)
        );
    }
}

#[test]
fn basic_stats_match_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    for input in ["tests/golden/stats.fa", "tests/golden/stats.fq"] {
        let our_output = run(&ours, &["stats", input]);
        let seqkit_output = run(Path::new("seqkit"), &["stats", "-T", input]);
        assert!(our_output.status.success());
        assert!(
            seqkit_output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&seqkit_output.stderr)
        );
        assert_eq!(our_output.stdout, seqkit_output.stdout, "input={input}");
    }
}

#[test]
fn alphabet_edge_cases_match_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    for (name, sequence) in [
        ("mixed-tu", "TU"),
        ("amino-oj", "OJ"),
        ("dna-redundant", "ACGTRYSWKMBDHVN"),
        ("rna-redundant", "ACGURYSWKMBDHVN"),
        ("unlimited", "ACGT?"),
    ] {
        let mut input = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
        writeln!(input, ">{name}\n{sequence}").unwrap();
        input.flush().unwrap();
        let path = input.path().to_str().unwrap();
        let our_output = run(&ours, &["stats", path]);
        let seqkit_output = run(Path::new("seqkit"), &["stats", "-T", path]);
        assert!(our_output.status.success(), "case={name}");
        assert!(
            seqkit_output.status.success(),
            "case={name}, stderr={}",
            String::from_utf8_lossy(&seqkit_output.stderr)
        );
        assert_eq!(our_output.stdout, seqkit_output.stdout, "case={name}");
    }
}

#[test]
fn literal_grep_modes_match_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    let cases: &[(&[&str], &[&str])] = &[
        (
            &["grep", "--pattern", "seq1", "tests/golden/records.fa"],
            &["grep", "-w", "0", "-p", "seq1", "tests/golden/records.fa"],
        ),
        (
            &[
                "grep",
                "--by-name",
                "--pattern",
                "seq2 beta",
                "tests/golden/records.fa",
            ],
            &[
                "grep",
                "-w",
                "0",
                "-n",
                "-p",
                "seq2 beta",
                "tests/golden/records.fa",
            ],
        ),
        (
            &[
                "grep",
                "--by-seq",
                "--pattern",
                "GGGGTTTT",
                "tests/golden/records.fa",
            ],
            &[
                "grep",
                "-w",
                "0",
                "-s",
                "-p",
                "GGGGTTTT",
                "tests/golden/records.fa",
            ],
        ),
        (
            &[
                "grep",
                "--ignore-case",
                "--invert-match",
                "--pattern",
                "SEQ3",
                "tests/golden/records.fa",
            ],
            &[
                "grep",
                "-w",
                "0",
                "-i",
                "-v",
                "-p",
                "SEQ3",
                "tests/golden/records.fa",
            ],
        ),
    ];
    for (our_args, seqkit_args) in cases {
        let our_output = run(&ours, our_args);
        let seqkit_output = run(Path::new("seqkit"), seqkit_args);
        assert!(
            our_output.status.success(),
            "ours stderr={}",
            String::from_utf8_lossy(&our_output.stderr)
        );
        assert!(
            seqkit_output.status.success(),
            "seqkit stderr={}",
            String::from_utf8_lossy(&seqkit_output.stderr)
        );
        assert_eq!(our_output.stdout, seqkit_output.stdout, "args={our_args:?}");
    }
}

#[test]
fn literal_grep_parser_and_strand_edges_match_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    let mut fasta = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
    write!(
        fasta,
        ">tabbed\tdescription\nACGT\n>rna\nAAAACCCU\n>protein\nMKWVTF\n"
    )
    .unwrap();
    fasta.flush().unwrap();
    let fasta_path = fasta.path().to_string_lossy().into_owned();
    let mut rna = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
    write!(rna, ">rna\nAAAACCCU\n").unwrap();
    rna.flush().unwrap();
    let rna_path = rna.path().to_string_lossy().into_owned();
    let cases = [
        (
            vec![
                "grep".into(),
                "--pattern".into(),
                "tabbed".into(),
                fasta_path.clone(),
            ],
            vec![
                "grep".into(),
                "-w".into(),
                "0".into(),
                "-p".into(),
                "tabbed".into(),
                fasta_path.clone(),
            ],
        ),
        (
            vec![
                "grep".into(),
                "--by-name".into(),
                "--pattern".into(),
                "tabbed\tdescription".into(),
                fasta_path.clone(),
            ],
            vec![
                "grep".into(),
                "-w".into(),
                "0".into(),
                "-n".into(),
                "-p".into(),
                "tabbed\tdescription".into(),
                fasta_path.clone(),
            ],
        ),
        (
            vec![
                "grep".into(),
                "--by-seq".into(),
                "--ignore-case".into(),
                "--pattern".into(),
                "gggu".into(),
                rna_path.clone(),
            ],
            vec![
                "grep".into(),
                "-w".into(),
                "0".into(),
                "-s".into(),
                "-i".into(),
                "-p".into(),
                "gggu".into(),
                rna_path.clone(),
            ],
        ),
        (
            vec![
                "grep".into(),
                "--by-seq".into(),
                "--only-positive-strand".into(),
                "--pattern".into(),
                "GGGU".into(),
                rna_path.clone(),
            ],
            vec![
                "grep".into(),
                "-w".into(),
                "0".into(),
                "-s".into(),
                "-P".into(),
                "-p".into(),
                "GGGU".into(),
                rna_path.clone(),
            ],
        ),
        (
            vec![
                "grep".into(),
                "--pattern".into(),
                "tabbed".into(),
                "--pattern".into(),
                "protein".into(),
                fasta_path.clone(),
            ],
            vec![
                "grep".into(),
                "-w".into(),
                "0".into(),
                "-p".into(),
                "tabbed".into(),
                "-p".into(),
                "protein".into(),
                fasta_path.clone(),
            ],
        ),
    ];
    for (our_args, seqkit_args) in cases {
        let our_output = run_strings(&ours, &our_args);
        let seqkit_output = run_strings(Path::new("seqkit"), &seqkit_args);
        assert!(
            our_output.status.success(),
            "ours args={our_args:?}, stderr={}",
            String::from_utf8_lossy(&our_output.stderr)
        );
        assert!(
            seqkit_output.status.success(),
            "seqkit args={seqkit_args:?}, stderr={}",
            String::from_utf8_lossy(&seqkit_output.stderr)
        );
        assert_eq!(
            our_output.stdout, seqkit_output.stdout,
            "ours={our_args:?}, seqkit={seqkit_args:?}"
        );
    }
}

#[test]
fn wrapped_fastq_conversion_matches_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    let mut input = tempfile::Builder::new().suffix(".fq").tempfile().unwrap();
    write!(input, "@wrapped\tdescription\nAC\nGT\n+\nII\nII\n").unwrap();
    input.flush().unwrap();
    let path = input.path().to_str().unwrap();

    let our_fasta = run(&ours, &["convert", "--to", "fasta", path]);
    let seqkit_fasta = run(Path::new("seqkit"), &["fq2fa", "-w", "0", path]);
    assert!(our_fasta.status.success());
    assert!(seqkit_fasta.status.success());
    assert_eq!(our_fasta.stdout, seqkit_fasta.stdout);

    let our_fastq = run(&ours, &["convert", "--to", "fastq", path]);
    let seqkit_fastq = run(Path::new("seqkit"), &["seq", "-w", "0", path]);
    assert!(our_fastq.status.success());
    assert!(seqkit_fastq.status.success());
    assert_eq!(our_fastq.stdout, seqkit_fastq.stdout);
}

#[test]
fn fastq_to_fasta_matches_live_seqkit() {
    if !require_or_skip() {
        return;
    }
    let ours = PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"));
    let our_output = run(
        &ours,
        &["convert", "--to", "fasta", "tests/golden/stats.fq"],
    );
    let seqkit_output = run(
        Path::new("seqkit"),
        &["fq2fa", "-w", "0", "tests/golden/stats.fq"],
    );
    assert!(our_output.status.success());
    assert!(
        seqkit_output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&seqkit_output.stderr)
    );
    assert_eq!(our_output.stdout, seqkit_output.stdout);
}
