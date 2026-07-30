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
