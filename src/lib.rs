#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! FASTA/FASTQ operations exposed by the `rsomics-seq` product.

mod input;
mod operations;

pub use operations::kmers::{KmerOptions, KmerReport, KmerRow, count_kmers, write_kmer_tsv};
pub use operations::stats::{SeqType, StatsReport, StatsRow, compute_stats, write_stats_tsv};
