#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <representative.fa-or-fq>" >&2
  exit 2
fi

fixture=$1
for tool in hyperfine seqkit; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 2
  fi
done

target_dir=${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR to allowed external build storage}
binary=$target_dir/release/rsomics-seq
if [[ ! -x "$binary" ]]; then
  echo "build $binary first" >&2
  exit 2
fi

checksum=$(shasum -a 256 "$fixture" | awk '{print $1}')
echo "fixture=$fixture"
echo "sha256=$checksum"
echo "seqkit=$(seqkit version 2>&1)"
uname -a

printf -v binary_q '%q' "$binary"
printf -v fixture_q '%q' "$fixture"
hyperfine --warmup 3 --runs 10 \
  "$binary_q stats $fixture_q >/dev/null" \
  "seqkit stats -T $fixture_q >/dev/null"

echo "This scaffold records wall time only; collect peak RSS before a release decision." >&2
