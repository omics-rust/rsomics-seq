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
