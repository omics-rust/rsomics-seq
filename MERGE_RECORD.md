# Strand-extraction consolidation record

Status: 0.3.0 native delivery candidate; operation engines frozen at
`3ba1818e85477573995f6a391522f2ef35054c79`. The original implementation used owner revision
`d9734e51c4ed557f6d8790d97a686717ebc4769e`; it made no version bump, publication, or
new performance claim at that stage. This extends the first-slice provenance in
[`PROVENANCE.md`](PROVENANCE.md).

## Team-owned implementation assets

| Historical repository | Exact revision | Selected assets and disposition |
|---|---|---|
| `rsomics-fasta-subseq` | `2f0bc3e920a429c36af7a1fb5cb45b7fdf47c802` | Refactor signed region validation/resolution into private-coordinate `Region`; retain regional and tab-header golden fixtures; discard FASTA-only/manual-output shell and UTF-8 header conversion |
| `rsomics-fasta-utils` | `93e81c2ab88524f97bdaaab6f34105743d798b96` | Refactor record-sized reverse-complement scratch and case-preserving DNA/RNA/IUPAC complement rules into the product |
| `rsomics-fastq-utils` | `7f0551d463977e6343af3c9477879799d1bcbc81` | Retain reverse-quality semantics and RNA FASTQ golden; replace per-record allocations and manual output |

Copied `tests/golden/subseq/*.fa` are unchanged historical assets from the
first repository above; `tests/golden/revcomp/*.fq` are unchanged assets from
the third. The historical region suite identifies SeqKit 2.13.0 as its oracle,
while its tab-header regression comments also reference 2.9.0; these are
retained historical witnesses, not newly generated 2.13.0 output. The RNA
FASTQ golden identifies SeqKit 2.13.0. Current tests additionally compare
whole records with a real installed oracle, not sequence fields alone.

## Current behavior witness and source checks

The observed local executable `/opt/homebrew/bin/seqkit` reports
`seqkit v2.14.0`. Complete stdin-driven FASTA/FASTQ outputs were inspected
before implementing the operations. Live test commands are:

```text
seqkit subseq -w 0 -r START:END [-R]
seqkit seq -r -p -w 0
```

`tests/seqkit_compat.rs` requires an available SeqKit for both new differential
tests; absence is a failure, not a skipped pass. The subsequence differential
checks 32 complete-output cases across two formats, eight signed/clamped/
empty ranges and coordinate-header mode. The reverse-complement differential
checks seven complete-output cases covering DNA, RNA, IUPAC case, protein,
unclassified and empty first records, later-record alphabet behavior and
nonuniform FASTQ quality reversal. The existing CI remains pinned to 2.13.0;
no new exact-head CI result is claimed by these local tests.

Source inspected:

- [SeqKit 2.13.0 regional route](https://github.com/shenwei356/seqkit/blob/v2.13.0/seqkit/cmd/subseq.go)
  and [2.14.0 regional route](https://github.com/shenwei356/seqkit/blob/v2.14.0/seqkit/cmd/subseq.go).
- [SeqKit 2.13.0 sequence route](https://github.com/shenwei356/seqkit/blob/v2.13.0/seqkit/cmd/seq.go)
  and [2.14.0 sequence route](https://github.com/shenwei356/seqkit/blob/v2.14.0/seqkit/cmd/seq.go).
- Their pinned `bio` dependencies' header parsing:
  [v0.13.9](https://github.com/shenwei356/bio/blob/v0.13.9/seqio/fastx/reader.go)
  and [v0.15.2](https://github.com/shenwei356/bio/blob/v0.15.2/seqio/fastx/reader.go).

The relevant region-record rewrite, reverse/complement and quality-output
routes agree between these sources. The default ID parser in both dependency
versions advances its description cursor twice when skipping a separator.
Consequently `one\t desc  more` becomes `one:1-3 esc  more` under `-R`.
This explicit header-rewrite option retains that source behavior; default
headers remain unchanged. Headers outside the existing `rsomics-seqio`
printable-ASCII-plus-tab domain remain invalid.

Region arithmetic uses wider signed intermediates so valid `i64` endpoints
cannot overflow while resolving against a record length. Sequence and quality
buffers are reused in reverse complement. Existing grep alphabet and search
semantics are unchanged; no foundation API or dependency is added.

## Verification boundary

Connected CLI tests were added first and failed because `subseq`/`revcomp`
were absent. An initial overbroad non-ASCII-header test then exposed the
existing strict seqio domain; the test was corrected to preserve accepted
headers and explicitly assert non-ASCII rejection, without changing seqio.

Local formatting, strict all-target/all-feature Clippy, and complete debug and
release suites exercise this source slice. Native Linux/macOS on both
architectures and representative per-operation/workflow CPU, memory and I/O
gates are subsequent delivery requirements. No historical performance result
is inherited as a pass, and no publication is authorized by this record.

## 0.3 delivery boundary

Native implementation CI run 41424424381 passed all four native classes.
The full 6,282,141-read campaign accepted the resource-use gates; standalone
subseq/revcomp wall medians lost 2.57%/5.72% and timing ranges overlapped.
This delivery changes package identity and delivery files only, not production
Rust or dependency resolution. Its executables are not newly benchmarked.
The owner-private helper follows the accepted rsomics-count/BED source-package,
license-inventory, fresh-install and native-bundle pattern. Delivery CI must
pass again at the final exact head. Registry publication remains held and its
existing workflow is unchanged.
