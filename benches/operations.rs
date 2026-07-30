use std::io::Write;

use criterion::{Criterion, criterion_group, criterion_main};
use rsomics_seq::{KmerOptions, compute_stats, count_kmers};

fn fixture(records: usize) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new().suffix(".fa").tempfile().unwrap();
    for index in 0..records {
        writeln!(
            file,
            ">record-{index}\nACGTACGTACGTACGTACGTACGTACGTACGTNNACGTACGTACGTACGT"
        )
        .unwrap();
    }
    file.flush().unwrap();
    file
}

fn operations(c: &mut Criterion) {
    let fixture = fixture(100_000);
    let input = fixture.path().to_string_lossy().into_owned();

    c.bench_function("stats_100k_fasta_records", |b| {
        b.iter(|| compute_stats(std::slice::from_ref(&input)).unwrap());
    });
    c.bench_function("kmers_k21_100k_fasta_records", |b| {
        b.iter(|| {
            count_kmers(
                &input,
                KmerOptions {
                    k: 21,
                    canonical: true,
                    min_count: 1,
                },
            )
            .unwrap()
        });
    });
}

criterion_group!(benches, operations);
criterion_main!(benches);
