#!/usr/bin/env bash
# Fetch, verify and convert the pinned Simple English Wikipedia snapshot (Feature 008).
#
# Usage: scripts/fetch-wiki.sh
#
# The parquet is checked against reference/datasets/wiki-manifest.json — size AND sha256 — and
# then converted to JSONL by reference/wiki_to_jsonl.py (the pinned .venv-008), which is checked
# the same way. A mismatch prints the path and both hashes and exits 1; nothing is retried from
# another source and nothing proceeds with a warning (spec FR-001). Idempotent: a cached parquet
# is not downloaded again and an existing JSONL is not reconverted, but both are re-verified.
#
# First run: the manifest's jsonl.sha256 is empty. The script then prints the measured bytes,
# sha256, line count and the converter's sha256 and exits 1 — pinning them in the manifest is a
# human action (research D2), never something this script does on its own.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$repo_root/reference/datasets/wiki-manifest.json"
cache="$repo_root/reference/datasets/wiki"
venv_python="$repo_root/reference/.venv-008/bin/python"
converter="$repo_root/reference/wiki_to_jsonl.py"

for tool in curl jq shasum; do
    command -v "$tool" >/dev/null || { printf 'fetch-wiki: FAIL — %s not found on PATH\n' "$tool" >&2; exit 1; }
done
[ -x "$venv_python" ] || { printf 'fetch-wiki: FAIL — %s missing; run scripts/setup-reference-venv.sh 008\n' "$venv_python" >&2; exit 1; }
mkdir -p "$cache"

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

verify() { # path expected_bytes expected_sha
    local path="$1" want_bytes="$2" want_sha="$3" have_bytes have_sha
    have_bytes="$(wc -c < "$path" | tr -d ' ')"
    have_sha="$(sha "$path")"
    if [ "$have_bytes" != "$want_bytes" ] || [ "$have_sha" != "$want_sha" ]; then
        printf 'fetch-wiki: FAIL — %s\n  expected %s bytes sha256=%s\n  actual   %s bytes sha256=%s\n' \
            "$path" "$want_bytes" "$want_sha" "$have_bytes" "$have_sha" >&2
        exit 1
    fi
}

url="$(jq -er '.parquet.url' "$manifest")"
parquet="$cache/$(basename "$url")"
if [ ! -f "$parquet" ]; then
    printf 'fetch-wiki: downloading %s\n' "$url"
    if ! curl -sSL --fail --connect-timeout 20 --max-time 1800 \
            --retry 5 --retry-delay 5 --retry-all-errors -o "$parquet.part" "$url"; then
        rm -f "$parquet.part"
        printf 'fetch-wiki: FAIL — could not download %s after 6 attempts (network/source, not a hash problem)\n' "$url" >&2
        exit 1
    fi
    mv "$parquet.part" "$parquet"
else
    printf 'fetch-wiki: %s already cached\n' "$(basename "$parquet")"
fi
verify "$parquet" "$(jq -r '.parquet.bytes' "$manifest")" "$(jq -r '.parquet.sha256' "$manifest")"

jsonl="$cache/$(jq -r '.jsonl.file' "$manifest")"
if [ ! -f "$jsonl" ]; then
    printf 'fetch-wiki: converting to %s\n' "$(basename "$jsonl")"
    "$venv_python" "$converter" "$parquet" "$jsonl"
else
    printf 'fetch-wiki: %s already converted\n' "$(basename "$jsonl")"
fi

want_sha="$(jq -r '.jsonl.sha256' "$manifest")"
lines="$(wc -l < "$jsonl" | tr -d ' ')"
if [ -z "$want_sha" ]; then
    printf 'fetch-wiki: the manifest does not pin the JSONL yet. Measured:\n' >&2
    printf '  "bytes": %s, "sha256": "%s", "lines": %s, "converter_sha256": "%s"\n' \
        "$(wc -c < "$jsonl" | tr -d ' ')" "$(sha "$jsonl")" "$lines" "$(sha "$converter")" >&2
    printf 'fetch-wiki: FAIL — pin these in %s (a human action), then re-run\n' "$manifest" >&2
    exit 1
fi
verify "$jsonl" "$(jq -r '.jsonl.bytes' "$manifest")" "$want_sha"
[ "$lines" = "$(jq -r '.jsonl.lines' "$manifest")" ] || { printf 'fetch-wiki: FAIL — %s has %s lines, manifest says %s\n' "$jsonl" "$lines" "$(jq -r '.jsonl.lines' "$manifest")" >&2; exit 1; }
[ "$(sha "$converter")" = "$(jq -r '.jsonl.converter_sha256' "$manifest")" ] || { printf 'fetch-wiki: FAIL — %s changed since the JSONL was pinned (sha256 %s); re-pin deliberately\n' "$converter" "$(sha "$converter")" >&2; exit 1; }
printf 'fetch-wiki: PASS — parquet and %s verified (%s lines)\n' "$(basename "$jsonl")" "$lines"
