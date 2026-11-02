//! Product operations.

/// FASTA/FASTQ format conversion.
pub mod convert;
/// Literal record filtering.
pub mod grep;
/// Exact DNA k-mer counting.
pub mod kmers;
/// Record-preserving reverse complement.
pub mod revcomp;
/// Basic FASTA/FASTQ statistics.
pub mod stats;
/// Record-preserving regional extraction.
pub mod subseq;
/// Strict FASTA/FASTQ validation.
pub mod validate;
