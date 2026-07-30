use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rsomics-seq"))
}

fn reverse_complement(window: &[u8]) -> Vec<u8> {
    window
        .iter()
        .rev()
        .map(|base| match base.to_ascii_uppercase() {
            b'A' => b'T',
            b'C' => b'G',
            b'G' => b'C',
            b'T' => b'A',
            other => panic!("reference received invalid base {other:?}"),
        })
        .collect()
}

fn reference(
    sequences: &[&[u8]],
    k: usize,
    canonical: bool,
    min_count: u64,
) -> Vec<(Vec<u8>, u64)> {
    let mut counts = HashMap::new();
    for sequence in sequences {
        for window in sequence.windows(k) {
            if !window
                .iter()
                .all(|base| matches!(base.to_ascii_uppercase(), b'A' | b'C' | b'G' | b'T'))
            {
                continue;
            }
            let forward: Vec<u8> = window.iter().map(u8::to_ascii_uppercase).collect();
            let key = if canonical {
                forward.clone().min(reverse_complement(&forward))
            } else {
                forward
            };
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut rows: Vec<_> = counts
        .into_iter()
        .filter(|(_, count)| *count >= min_count)
        .collect();
    rows.sort_unstable_by(|(left_kmer, left_count), (right_kmer, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_kmer.cmp(right_kmer))
    });
    rows
}

fn parse_tsv(bytes: &[u8]) -> Vec<(Vec<u8>, u64)> {
    let text = std::str::from_utf8(bytes).unwrap();
    text.lines()
        .skip(1)
        .map(|line| {
            let (kmer, count) = line.split_once('\t').unwrap();
            (kmer.as_bytes().to_vec(), count.parse().unwrap())
        })
        .collect()
}

fn assert_matches(input: &Path, sequences: &[&[u8]], k: usize, canonical: bool, min_count: u64) {
    let mut command = Command::new(binary());
    command.args([
        "kmers",
        "-k",
        &k.to_string(),
        "--min-count",
        &min_count.to_string(),
    ]);
    if canonical {
        command.arg("--canonical");
    }
    let output = command
        .arg(input)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        parse_tsv(&output.stdout),
        reference(sequences, k, canonical, min_count)
    );
}

#[test]
fn foundation_counts_match_independent_window_oracle() {
    let fasta_sequences: &[&[u8]] = &[b"ACGTACGTNACGT", b"ACGT"];
    for canonical in [false, true] {
        assert_matches(
            Path::new("tests/golden/kmers.fa"),
            fasta_sequences,
            3,
            canonical,
            1,
        );
    }

    let mut fastq = tempfile::Builder::new().suffix(".fq").tempfile().unwrap();
    write!(
        fastq,
        "@one\nacgtacgt\n+\nIIIIIIII\n@two\nacgtncgt\n+\nIIIIIIII\n"
    )
    .unwrap();
    fastq.flush().unwrap();
    let fastq_sequences: &[&[u8]] = &[b"acgtacgt", b"acgtncgt"];
    assert_matches(fastq.path(), fastq_sequences, 1, false, 2);
    assert_matches(fastq.path(), fastq_sequences, 3, true, 2);

    let sequence = b"acgtacgtacgtacgtacgtacgtacgtacgt";
    let mut fasta = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
    writeln!(
        fasta,
        ">boundary\n{}",
        std::str::from_utf8(sequence).unwrap()
    )
    .unwrap();
    fasta.flush().unwrap();
    assert_matches(fasta.path(), &[sequence], 32, true, 1);
}
