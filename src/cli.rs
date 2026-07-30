use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use rsomics_common::{CommonFlags, Result, RsomicsError, ToolMeta};
use serde::Serialize;

use rsomics_seq::{
    KmerOptions, KmerReport, StatsReport, compute_stats, count_kmers, write_kmer_tsv,
    write_stats_tsv,
};

use crate::output::{reject_output_alias, with_output};

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
    pub common: CommonFlags,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report basic sequence-file statistics.
    Stats(StatsArgs),
    /// Count exact DNA k-mers.
    Kmers(KmerArgs),
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

#[derive(Debug, Serialize)]
#[serde(tag = "operation", content = "data", rename_all = "lowercase")]
pub enum CommandResult {
    Stats(StatsReport),
    Kmers(KmerReport),
}

impl Cli {
    pub fn execute(self) -> Result<CommandResult> {
        let json = self.common.json;
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
