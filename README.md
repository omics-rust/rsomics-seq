# rsomics-seq

`rsomics-seq` is the coherent FASTA/FASTQ utility product in the rsomics
family. Its first release scope contains five complete commands:

- `stats`: basic per-input sequence statistics compatible with
  `seqkit stats -T`;
- `kmers`: deterministic exact DNA k-mer counts over FASTA or FASTQ;
- `grep`: literal record filtering by ID, full name, or sequence;
- `convert`: FASTA/FASTQ normalization and FASTQ-to-FASTA conversion;
- `validate`: strict complete-stream FASTA/FASTQ validation.

All commands accept plain, gzip, or BGZF input detected from content.
`-` reads stdin.

## Usage

```text
rsomics-seq stats assembly.fa reads.fq.gz
cat assembly.fa | rsomics-seq stats -
rsomics-seq kmers -k 21 --canonical --min-count 2 assembly.fa
rsomics-seq kmers -k 15 reads.fq.gz --json
rsomics-seq grep -p chrM assembly.fa
rsomics-seq grep --by-seq -p ACGT reads.fq.gz
rsomics-seq convert --to fasta reads.fq.gz
rsomics-seq validate assembly.fa
```

`stats`, `kmers`, and `validate` text reports are TSV. `grep` and `convert`
write FASTA/FASTQ records. `--json` suppresses record/TSV output and emits the
versioned `rsomics-common` report envelope to stdout; combining it with a file
`--output` is a configuration error with exit code 2.

Named outputs are written to a same-directory temporary file and atomically
persisted only after the operation succeeds. Exact, normalized, hard-link, and
symbolic-link aliases of any input are rejected before reading or truncating
data. New outputs use normal `0666 & !umask` permissions; replacements preserve
the existing permission bits.
Compressed named output is not implemented in this release; `.gz`, `.bgz`, and
`.bgzf` output names are rejected instead of receiving uncompressed bytes.

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

### `grep`

- ID mode is the default and matches the token before the first ASCII
  whitespace by whole-string equality.
- `--by-name` matches the complete header by whole-string equality.
- `--by-seq` performs literal substring matching. DNA/RNA input searches both
  strands by default; `--only-positive-strand` disables the reverse-complement
  search. Protein or unclassified input is searched only on the positive
  strand.
- `--ignore-case`, `--invert-match`, repeated `--pattern`, and comma-separated
  patterns are supported.
- Selected records retain input order and input format.

Regexes, pattern files, degenerate expansion, mismatch matching, region
restriction, circular matching, duplicate emission, and record-count limiting
are explicitly outside this release scope.

### `convert`

- Same-format conversion normalizes records through the strict
  `rsomics-seqio` writer.
- FASTQ-to-FASTA conversion removes quality scores.
- FASTA-to-FASTQ conversion fails non-zero because the command never invents
  quality scores. SeqKit's separate `fa2fq` lookup workflow is not represented
  as a direct format conversion.
- Header rewriting, sequence transforms, tabular `fx2tab` output, and dummy
  quality generation are outside this release scope.

### `validate`

Validation parses the complete stream with `rsomics-seqio` and reports the
detected format and record count only after success. Format, decompression,
record, and I/O errors propagate non-zero. Validation does not silently repair,
skip, or rewrite malformed records.

## Compatibility and evidence

Committed FASTA and FASTQ goldens were captured from SeqKit and run on every
test platform. CI installs SeqKit v2.13.0 and requires live differentials for
basic stats, literal ID/name/sequence grep modes, and FASTQ-to-FASTA conversion.
K-mer counting is checked against a separate byte-window reference
implementation, including canonicalization and ambiguity runs.

The release gate runs on native Linux and macOS for both `x86_64` and
`aarch64`. A representative Linux `x86_64` gate used 6,282,141 compressed
SRR341550 reads. Full `stats`, ID grep, sequence grep, FASTQ-to-FASTA, and
FASTQ normalization outputs matched SeqKit 2.13.0 byte for byte. Canonical
21-mer counts over a 100,000-read subset matched Jellyfish 2.3.1 for all
104,521 emitted rows. A malformed FASTQ failed after its valid prefix and did
not commit the named report.

On that host, `stats` and double-strand sequence grep were 1.32 and 1.82 times
faster than their SeqKit counterparts while using substantially less peak
memory. Conversion throughput was equal or slower, but peak memory remained
68–91% lower. Exact k-mer counting was 1.52 times slower than the matched
Jellyfish count/dump/sort pipeline and used 63% less peak memory. These are
explicit throughput/resource tradeoffs, not a blanket replacement claim.
Exact commands, distributions, RSS, checksums, and limitations are recorded in
[`PERFORMANCE.md`](PERFORMANCE.md).

The current commands are streaming operations and do not advertise a thread
count. Compressed-path concurrency comes from the fixed reader/decompressor
pipeline; speculative parallel implementations are outside this release.

## Current limitations

- Extended `seqkit stats --all` columns are not yet exposed.
- `kmers` is an exact in-memory counter and is not intended for
  cardinalities that require a disk-backed counter.
- `rsomics-common 0.7`, `rsomics-help 0.4`, `rsomics-seqio 0.3`, and
  `rsomics-kmer 0.2.1` are not published yet. The manifest uses versioned
  registry dependencies; CI temporarily patches exact revisions without
  committing path dependencies.
- Native Linux `aarch64` has correctness and compatibility CI but no
  representative performance measurement.

## Origin and license

The implementation consolidates team-owned historical rsomics code. Exact
source revisions, retained evidence, and behavior changes are recorded in
[`PROVENANCE.md`](PROVENANCE.md).

SeqKit is MIT licensed. `rsomics-seq` is licensed under MIT OR Apache-2.0.
