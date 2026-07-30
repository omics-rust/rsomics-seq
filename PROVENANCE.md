# Migration and compatibility record

## First consumer slice

| Target | Historical source | Revision | Disposition |
|---|---|---|---|
| `stats` | `rsomics-fasta-stats` | `57106ebba455a7eff5ce9b0958826965166dd4eb` | refactor into format-generic streaming operation |
| `stats` | `rsomics-fastq-stats` | `bd19eaa7ddcd43430c09fa4ecd735308931496cc` | retain basic columns and first-record alphabet rule |
| `stats` evidence | `rsomics-seq-stats` | `895a28e50f4cab0df247921afced966a15bbd0a5` | fixtures and frozen SeqKit output only |
| `kmers` | `rsomics-fasta-utils` | `93e81c2ab88524f97bdaaab6f34105743d798b96` | refactor naive allocating windows through `rsomics-kmer` |
| `grep` | `rsomics-seq-grep` | `5bef2f6f9009b47c188262f96b0b6a5976bc3a84` | refactor literal ID, name, and sequence filtering into the product |
| `convert` | `rsomics-fastx-convert` | `6ac272a61a7017ce0bf26b2209fbea113fb561a1` | retain format-conversion intent; replace permissive parsing and dummy-quality behavior |
| `convert` evidence | `rsomics-fasta-fx2tab` | `40031f6915b2a54c4cf82d3011aec8a66d064b39` | command-boundary and test asset only; tabular conversion excluded |
| `validate` | `rsomics-fasta-validate` | `93da2fdc4f8596899e76822271ab6a718df519e6` | replace record-local checks with a strict full-stream scan |
| `validate` | `rsomics-fastq-validate` | `69f6af79d6af37a6384ac3af3938062d11c60ae7` | replace record-local checks with a strict full-stream scan |

Foundation revisions exercised by this slice:

- `rsomics-common` `1c51f7d0b356683697942d9c6a0f60585e0dc8a9`
- `rsomics-seqio` `c42f4212b78516cf3e756376d3581d08aff59879`
- `rsomics-kmer` `4258ac881119bcee69a3541119bb3e544500743a`
- `rsomics-common` `ed02bcb9f813`

All listed rsomics implementations are team-owned.

## Behavior retained

- Stats basic column names, order, one-decimal average length, and
  first-record alphabet classification follow the historical SeqKit-backed
  implementations.
- K-mer rows retain descending-count, lexicographic-tie ordering.
- K-mer windows containing non-ACGT bytes are skipped.
- Grep preserves input order and format and retains literal whole-ID,
  whole-name, and sequence-substring modes.
- FASTQ-to-FASTA conversion retains headers and sequences while dropping
  quality scores.

## Behavior changed

- Both operations now use strict `rsomics-seqio` FASTA/FASTQ parsing,
  content-based compression detection, and stdin support.
- `stats` is format-generic instead of maintaining separate FASTA and FASTQ
  binaries.
- `kmers` accepts FASTA and FASTQ and uses the shared rolling/two-bit
  foundation. `k` is therefore explicitly limited to `1..=32`.
- `kmers --canonical` is an explicit supported mode.
- Empty input and malformed records fail non-zero; permissive parser recovery
  from historical tools is not retained.
- JSON uses the shared `rsomics-common` envelope.
- Named output is transactional and rejects aliases of every input before
  processing. JSON/file-output conflicts use the common configuration-error
  contract.
- User-supplied k-mer lengths are validated through a fallible product boundary
  before the foundation accumulator is constructed.
- Grep's first release surface intentionally excludes regexes, pattern files,
  regions, mismatch and degenerate matching, circular matching, and duplicate
  emission. DNA/RNA sequence matching follows SeqKit's default two-strand
  behavior with an explicit positive-only option.
- Convert never synthesizes FASTQ quality scores. FASTA-to-FASTQ therefore
  fails instead of retaining historical dummy-quality behavior.
- Validate is format-generic and succeeds only after `rsomics-seqio` parses the
  complete stream. It does not repair or skip malformed records.

## Oracles

- Stats frozen goldens: SeqKit v2.9.0 historical assets, rechecked against
  SeqKit v2.13.0 for the retained basic fields.
- Stats live command:
  `seqkit stats -T <input>`.
- Grep live commands:
  `seqkit grep -w 0 -p <pattern> <input>`,
  with `-n`, `-s`, `-i`, `-v`, or `-P` added for the corresponding retained
  modes.
- FASTQ-to-FASTA live command:
  `seqkit fq2fa -w 0 <input>`.
  CI pins SeqKit v2.13.0 and fails rather than skipping when it is unavailable.
- K-mer oracle: an independent direct byte-window counter in
  `tests/kmers_oracle.rs`. It uppercases valid windows, skips windows with
  non-ACGT bytes, and computes reverse complements independently of
  `rsomics-kmer`.

No performance result is inherited as a pass. Historical timings used
different binaries and I/O stacks.

## Consumer-driven foundation feedback

This slice identified that `rsomics-kmer` declared `rsomics-common` without
using it and lacked a checked accumulator constructor for user-supplied `k`.
The pinned `4258ac881119bcee69a3541119bb3e544500743a` revision removes that
dependency, avoids pulling Rayon through the product, and provides the
fallible boundary used here.
