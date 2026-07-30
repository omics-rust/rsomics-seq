#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! FASTA/FASTQ operations exposed by the `rsomics-seq` product.

mod input;
mod operations;

pub use operations::convert::{ConvertFormat, ConvertReport, convert_sequences};
pub use operations::grep::{GrepMode, GrepOptions, GrepReport, grep_records};
pub use operations::kmers::{KmerOptions, KmerReport, KmerRow, count_kmers, write_kmer_tsv};
pub use operations::stats::{SeqType, StatsReport, StatsRow, compute_stats, write_stats_tsv};
pub use operations::validate::{ValidationReport, validate_sequences, write_validation_tsv};
