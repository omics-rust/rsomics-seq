# rsomics-seq

`rsomics-seq` is the coherent FASTA/FASTQ utility product in the rsomics
family. The first consumer slice contains two complete commands:

- `stats`: basic per-input sequence statistics compatible with
  `seqkit stats -T`;
- `kmers`: deterministic exact DNA k-mer counts over FASTA or FASTQ.

Both commands accept plain, gzip, or BGZF input detected from content.
`-` reads stdin.

## Usage

```text
rsomics-seq stats assembly.fa reads.fq.gz
cat assembly.fa | rsomics-seq stats -
rsomics-seq kmers -k 21 --canonical --min-count 2 assembly.fa
rsomics-seq kmers -k 15 reads.fq.gz --json
```

Text output is TSV. `--json` emits the versioned `rsomics-common` envelope to
stdout; combining it with a file `--output` is a configuration error with exit
code 2.

Named TSV outputs are written to a same-directory temporary file and atomically
persisted only after the operation succeeds. Exact, normalized, hard-link, and
symbolic-link aliases of any input are rejected before reading or truncating
data. New outputs use normal `0666 & !umask` permissions; replacements preserve
the existing permission bits.

## Stable command semantics

### `stats`

The basic columns are:

```text
file format type num_seqs sum_len min_len avg_len max_len
```

Alphabet classification follows SeqKit's first-record rule. Empty inputs are
errors. The strict `rsomics-seqio` parser rejects malformed records instead of
reproducing permissive recovery behavior from older micro-crates.

### `kmers`

- `k` is validated fallibly as `1..=32` before constructing the shared two-bit
  accumulator.
- Counting is case-insensitive over A/C/G/T.
- A window containing any other byte is skipped.
- `--canonical` collapses a k-mer and its reverse complement to the
  lexicographically smaller representation.
- Rows are sorted by descending count and then lexicographically.
- `--min-count` defaults to one and must be positive.

The JSON report includes candidate, valid, skipped, distinct, and emitted
window/count totals. TSV contains only `kmer` and `count`.

## Compatibility and evidence

Committed FASTA and FASTQ goldens were captured from SeqKit and run on every
test platform. CI installs SeqKit v2.13.0 and requires the live differential.
K-mer counting is checked against a separate byte-window reference
implementation, including canonicalization and ambiguity runs.

The Criterion benchmark is a local hot-path scaffold, not a performance
claim. `scripts/perf.sh` records the command shape for a future representative
SeqKit comparison. No operation is release-ready until timing distribution,
peak memory, fixture checksum, machine, compression, and thread provenance are
recorded.

## Current limitations

- Only `stats` and `kmers` are implemented. The dossier's `grep`, `convert`,
  and `validate` commands remain excluded from this slice.
- Extended `seqkit stats --all` columns are not yet exposed.
- `kmers` is an exact in-memory counter and is not intended for
  cardinalities that require a disk-backed counter.
- `rsomics-seqio 0.2.0` and `rsomics-kmer 0.2.1` are not published yet.
  The manifest uses versioned registry dependencies; local development uses
  external Cargo patch configuration rather than committed path dependencies.
- The current `rsomics-help` API duplicates command metadata instead of
  deriving nested help from Clap. This product keeps one Clap command tree and
  does not freeze a second help schema until the shared API is redesigned.

## Origin and license

The implementation consolidates team-owned historical rsomics code. Exact
source revisions, retained evidence, and behavior changes are recorded in
[`PROVENANCE.md`](PROVENANCE.md).

SeqKit is MIT licensed. `rsomics-seq` is licensed under MIT OR Apache-2.0.
