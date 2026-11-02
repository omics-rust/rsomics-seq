use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use rsomics_common::{OutputArgs, Result, RsomicsError, ToolMeta};
use serde::Serialize;

use rsomics_seq::{
    ConvertFormat, ConvertReport, GrepMode, GrepOptions, GrepReport, KmerOptions, KmerReport,
    Region, RevcompReport, StatsReport, SubseqReport, ValidationReport, compute_stats,
    convert_sequences, count_kmers, extract_subsequences, grep_records,
    reverse_complement_sequences, validate_sequences, write_kmer_tsv, write_stats_tsv,
    write_validation_tsv,
};

use crate::output::{reject_output_alias, with_output, with_sequence_output};

pub const META: ToolMeta = ToolMeta {
    name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
};

#[derive(Debug, Parser)]
#[command(
    name = "rsomics-seq",
    version,
    about = "Coherent FASTA/FASTQ sequence utilities",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report basic sequence-file statistics.
    Stats(StatsArgs),
    /// Count exact DNA k-mers.
    Kmers(KmerArgs),
    /// Select records by literal identifier, name, or sequence.
    Grep(GrepArgs),
    /// Normalize or convert FASTA/FASTQ records.
    Convert(ConvertArgs),
    /// Strictly parse and validate a complete FASTA/FASTQ stream.
    Validate(ValidateArgs),
    /// Extract signed, 1-based inclusive regions while preserving qualities.
    Subseq(SubseqArgs),
    /// Reverse-complement records and reverse FASTQ quality scores.
    Revcomp(RevcompArgs),
}

#[derive(Debug, Args)]
struct StatsArgs {
    /// FASTA/FASTQ inputs; `-` reads stdin.
    #[arg(default_value = "-", num_args = 1..)]
    inputs: Vec<String>,

    /// Write TSV data here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct KmerArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// K-mer length, constrained by the two-bit foundation codec.
    #[arg(short, long)]
    k: usize,

    /// Collapse each k-mer with its reverse complement.
    #[arg(short, long)]
    canonical: bool,

    /// Emit only k-mers observed at least this many times.
    #[arg(short = 'm', long, default_value_t = 1)]
    min_count: u64,

    /// Write TSV data here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct GrepArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Literal pattern; repeat the flag or use comma-separated values.
    #[arg(short, long = "pattern", required = true, value_delimiter = ',')]
    patterns: Vec<String>,

    /// Match the complete record header instead of its identifier.
    #[arg(short = 'n', long, conflicts_with = "by_seq")]
    by_name: bool,

    /// Match a literal substring of the sequence.
    #[arg(short = 's', long, conflicts_with = "by_name")]
    by_seq: bool,

    /// Ignore ASCII letter case.
    #[arg(short = 'i', long)]
    ignore_case: bool,

    /// Select records that do not match.
    #[arg(long)]
    invert_match: bool,

    /// Search only the positive strand in sequence mode.
    #[arg(short = 'P', long, requires = "by_seq")]
    only_positive_strand: bool,

    /// Write selected records here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct ConvertArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Target sequence format.
    #[arg(long, value_enum)]
    to: ConvertFormat,

    /// Write converted records here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct ValidateArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Write the validation report here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct SubseqArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Signed, nonzero START:END coordinates; negative values count from the end.
    #[arg(short = 'r', long, allow_hyphen_values = true)]
    region: String,

    /// Append requested coordinates to the ID using SeqKit header formatting.
    #[arg(short = 'R', long)]
    region_coord: bool,

    /// Write extracted records here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct RevcompArgs {
    /// FASTA/FASTQ input; `-` reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Write transformed records here; `-` writes stdout.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", content = "data", rename_all = "lowercase")]
pub enum CommandResult {
    Stats(StatsReport),
    Kmers(KmerReport),
    Grep(GrepReport),
    Convert(ConvertReport),
    Validate(ValidationReport),
    Subseq(SubseqReport),
    Revcomp(RevcompReport),
}

impl Cli {
    pub fn execute(self) -> Result<CommandResult> {
        let json = self.output.json;
        match self.command {
            Command::Stats(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, args.inputs.iter().map(std::path::Path::new))?;
                let report = compute_stats(&args.inputs)?;
                if !json {
                    with_output(&args.output, |output| write_stats_tsv(&report, output))?;
                }
                Ok(CommandResult::Stats(report))
            }
            Command::Kmers(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let report = count_kmers(
                    &args.input,
                    KmerOptions {
                        k: args.k,
                        canonical: args.canonical,
                        min_count: args.min_count,
                    },
                )?;
                if !json {
                    with_output(&args.output, |output| write_kmer_tsv(&report, output))?;
                }
                Ok(CommandResult::Kmers(report))
            }
            Command::Grep(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let mode = if args.by_seq {
                    GrepMode::Sequence
                } else if args.by_name {
                    GrepMode::Name
                } else {
                    GrepMode::Id
                };
                let options = GrepOptions {
                    patterns: args.patterns,
                    mode,
                    ignore_case: args.ignore_case,
                    invert_match: args.invert_match,
                    only_positive_strand: args.only_positive_strand,
                };
                let report = if json {
                    grep_records(&args.input, &options, &mut std::io::sink())?
                } else {
                    with_sequence_output(&args.output, |output| {
                        grep_records(&args.input, &options, output)
                    })?
                };
                Ok(CommandResult::Grep(report))
            }
            Command::Convert(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let report = if json {
                    convert_sequences(&args.input, args.to, &mut std::io::sink())?
                } else {
                    with_sequence_output(&args.output, |output| {
                        convert_sequences(&args.input, args.to, output)
                    })?
                };
                Ok(CommandResult::Convert(report))
            }
            Command::Validate(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let report = validate_sequences(&args.input)?;
                if !json {
                    with_output(&args.output, |output| write_validation_tsv(&report, output))?;
                }
                Ok(CommandResult::Validate(report))
            }
            Command::Subseq(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let region: Region = args.region.parse()?;
                let report = if json {
                    extract_subsequences(
                        &args.input,
                        region,
                        args.region_coord,
                        &mut std::io::sink(),
                    )?
                } else {
                    with_sequence_output(&args.output, |output| {
                        extract_subsequences(&args.input, region, args.region_coord, output)
                    })?
                };
                Ok(CommandResult::Subseq(report))
            }
            Command::Revcomp(args) => {
                validate_json_output(json, &args.output)?;
                reject_output_alias(&args.output, [std::path::Path::new(args.input.as_str())])?;
                let report = if json {
                    reverse_complement_sequences(&args.input, &mut std::io::sink())?
                } else {
                    with_sequence_output(&args.output, |output| {
                        reverse_complement_sequences(&args.input, output)
                    })?
                };
                Ok(CommandResult::Revcomp(report))
            }
        }
    }
}

fn validate_json_output(json: bool, output: &std::path::Path) -> Result<()> {
    if json && output != std::path::Path::new("-") {
        return Err(RsomicsError::ConfigError(
            "--json writes its envelope to stdout and cannot be combined with --output".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn command_tree_is_valid() {
        Cli::command().debug_assert();
    }
}
