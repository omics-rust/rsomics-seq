# Representative compatibility and performance gate

Status: the first five-command slice has representative Linux `x86_64`
evidence and exact-head four-native-target CI. It is not approved for
publication.

## Exact identities

- product source:
  `d1369a5fe8cbb2d77f847242ac569d11c7777bc8`;
- product binary SHA-256:
  `a9a7e2d78b30499a2c635900631197b2c0adc33179947da1a53e188d22069e23`;
- `rsomics-common`:
  `1c51f7d0b356683697942d9c6a0f60585e0dc8a9`;
- `rsomics-seqio`:
  `ce9c5514c23573a64406e1ff9ad02edfa4d02d31`;
- `rsomics-kmer`:
  `4258ac881119bcee69a3541119bb3e544500743a`;
- Rust: 1.91.0;
- SeqKit: 2.13.0, source
  `d13b5fa388cc869de05abe1bdb07980eef5efb4e`, binary SHA-256
  `68e55e64ca2c5123376c87dbee8f69cf3e2d41bada0639a9b7d7d56de73eea04`;
- Jellyfish: 2.3.1, binary SHA-256
  `3e5179f41af2d286019f828747ee7ff0f9b92fd742d8ce1b70f56746a96e4098`;
- exact-head CI run: `30545457538`.

The CI run passed formatting, strict Clippy, all tests, live SeqKit
differentials, and benchmark smoke on native Ubuntu and macOS for both
`x86_64` and `aarch64`.

## Host and inputs

Measurements ran on `dell-Precision-7920-Tower`, Ubuntu 22.04, Linux 6.8,
`x86_64`, with two Intel Xeon Gold 6238R CPUs. Source, binaries, fixtures,
temporary files, and results remained under
`/data1/liangjy/rsomics-linux-x86_64-20260730`.

The full input was `SRR341550_1.fastq.gz`, SHA-256
`d7a15c1762d64a5434ced0cc665d7f5d167ca81a71e239f8237b9cd490dd7683`,
containing 6,282,141 reads. The first 100,000 reads formed the k-mer input,
SHA-256
`2b5e03ad577059d432c1ce986bf1a850bc6402b4f17ef7f41015278e4bfb18a3`.

Commands were pinned with `taskset` to physical cores 48–51 on one socket.
Hyperfine used one warmup and ten measured runs, except the k-mer pair, which
used five measured runs. Timed record output went to `/dev/null`; correctness
was checked separately on materialized outputs. Peak RSS came from separate
GNU `time -v` runs. Other users had jobs on the host, so core binding and the
observed distributions are part of the provenance rather than an assumption
of an idle machine.

## Correctness

| Operation | Oracle | Result |
|---|---|---|
| `stats` | `seqkit stats -T` | full TSV byte-identical, SHA-256 `5f78d46bfcfe61c7af4ec8732ce01c6592b1d5c0e2f467a2f1e9cdb8ea78c555` |
| ID `grep` | `seqkit grep -w 0 -p SRR341550.1` | FASTQ byte-identical, SHA-256 `0743f36c0d33639d68ab224006c59596d42a85ad372db1bcc7498dc7d6349085` |
| sequence `grep` | two-strand SeqKit search for `AGACGTGTGCTCTTCCGATCT` | 17,100 records byte-identical, SHA-256 `cd43d3a2c3cc863cb759779829844b4f544a6cecf2c429376bac41dce1eb153a` |
| FASTQ to FASTA | `seqkit fq2fa -w 0` | full output byte-identical, SHA-256 `a66e0217083efc3d2929184490e9f394409270bbdb2f3c4d33884553143a1ebd` |
| FASTQ normalization | `seqkit seq -w 0` | full output byte-identical, SHA-256 `1ffbf697b7c153da31d89d04031fcbca5010f5dd7cf3a00b55d58b8381a4841e` |
| `validate` | strict parser and malformed fixture | complete valid stream accepted; truncated quality failed with exit 1 after record 1 and did not commit the named output |
| canonical k=21 | Jellyfish `count -C`, `dump -L 2`, normalized ordering | all 104,521 emitted rows byte-identical, SHA-256 `85e5df3a42f5c2526cdf1cfd86a53709018ff3a43d9398b9975116592fe8ba78` |

The k-mer fixture contained 8,100,000 candidate windows, 8,099,205 valid
windows, 795 ambiguity-bearing skipped windows, and 543,158 distinct
canonical k-mers before the minimum-count filter.

## Performance

Times are arithmetic means and sample standard deviations. RSS is KiB.

| Operation | Runs | rsomics | Oracle/reference | RSS, rsomics / reference | Decision |
|---|---:|---:|---:|---:|---|
| `stats` | 10 | 1.143 ± 0.045 s | SeqKit 1.513 ± 0.015 s | 6,732 / 22,400 | 1.32 times faster and 70% lower RSS |
| ID `grep` | 10 | 1.400 ± 0.043 s | SeqKit 1.666 ± 0.120 s | 7,100 / 58,240 | 1.19 times faster and 88% lower RSS; SeqKit run contained outliers |
| sequence `grep` | 10 | 5.919 ± 0.036 s | SeqKit 10.763 ± 0.109 s | 6,748 / 132,608 | 1.82 times faster and 95% lower RSS |
| FASTQ to FASTA | 10 | 1.616 ± 0.123 s | SeqKit 1.584 ± 0.087 s | 6,892 / 21,504 | throughput tied within noise; 68% lower RSS |
| FASTQ normalization | 10 | 1.692 ± 0.019 s | SeqKit 1.510 ± 0.040 s | 7,088 / 75,264 | 12% slower; 91% lower RSS |
| `validate` | 10 | 1.102 ± 0.009 s | no semantic-equivalent oracle | 6,736 / not applicable | standalone strict-scan baseline only |
| canonical k=21 | 5 | 0.754 ± 0.007 s | Jellyfish 0.495 ± 0.006 s | 29,056 / 78,848 | 1.52 times slower; 63% lower RSS |

The Jellyfish timing includes count, dump, and count/lexicographic sorting so
that both sides perform the user-visible deterministic table work. Its
temporary database was 5,433,132 bytes.

This gate supports throughput claims only for `stats` and the measured grep
modes. Conversion and exact k-mer counting are retained as explicit
resource/strictness tradeoffs, not throughput wins.

## Thread-control finding

The product accepts the shared `--threads` flag, but its current operations do
not use the configured Rayon pool. Compressed path input uses one decompression
worker feeding the streaming parser. A five-run control measured:

- `stats --threads 1`: 1.118 ± 0.021 s;
- `stats --threads 4`: 1.110 ± 0.007 s;
- sequence grep `--threads 1`: 5.945 ± 0.037 s;
- sequence grep `--threads 4`: 6.279 ± 0.363 s.

There is no demonstrated scaling. The CPU affinity is therefore described as
a four-core allocation, not a four-thread product mode. Before publication,
the shared CLI must either make the flag control a concrete operation or stop
advertising it for this product. This finding does not justify speculative
parallel code or a new public foundation API.

## Raw evidence

Remote result directory:
`/data1/liangjy/rsomics-linux-x86_64-20260730/results/seq-gate-run2`.

Principal Hyperfine JSON SHA-256 values:

- stats:
  `a0d9cbb5d0785a9332eb910a6ff41971a26b13bc6aeec84ddc4c436a4626e397`;
- ID grep:
  `c21008073fce2b59721c9980ba090a4deed681672177b5839bf9ee58fba5b505`;
- sequence grep:
  `9d06736fb263496d9f22d3c29726724ea1e6931854d776015080d4c35c904af5`;
- FASTQ to FASTA:
  `08e728e783c67709706995cb5ed2577bd7238a759115fca45bcf0fe371e47967`;
- FASTQ normalization:
  `6f888649d4bc0ba6f2f00f92c81b2d5d318bb8c80a39cea10a7fe481e150bb60`;
- validate:
  `862629dffbd11052e7a93dbeeeb79838c5cecfcf860c362170c17036c6e04d26`;
- k-mers:
  `4be3c426bf2dcdc9d033a3c2444d3188f0868375dd89174fcbaaac0020e03136`;
- stats thread control:
  `3c256bbd82fea6a8ed3829b797e36e30475bb2ac3d0918bb786ca16cc1208efb`;
- sequence-grep thread control:
  `446bb19746e341bbd5b96c7791ba6960bab9cd41ab4676a669ef44d8b04973b9`.

The correctness metadata and checksum ledgers have SHA-256 values
`3b1ee35d9b266019b23eb9c54fc4bcf407a1efb923f801900e9089be280bcd66`
and
`f1c519e22d1d5c272060f8be5b52efb67f1116130637ef5307e7824ef80af7c9`.

## Remaining gates

- Native Linux `aarch64` has CI correctness evidence but no representative
  performance host.
- The `--threads` product contract remains unresolved.
- The referenced foundation versions are not published.
- Final public API and hot-path review remain.
