#!/usr/bin/env bash
# Fetch, verify and extract the pinned BEIR datasets for xtriever-eval (Feature 003).
#
# Usage: scripts/fetch-beir.sh [scifact] [nfcorpus] [fiqa]     (default: all three)
#
# Every archive and every file the harness reads is checked against reference/datasets/
# beir-manifest.json — size AND sha256. A mismatch prints the path and both hashes and exits 1;
# nothing is retried from another source and nothing proceeds with a warning (spec FR-007).
# Idempotent: an archive already present is not downloaded again, but everything is re-verified.
# The cache directory is git-ignored (FR-008).

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$repo_root/reference/datasets/beir-manifest.json"
cache="$repo_root/reference/datasets/beir"

for tool in curl jq unzip shasum; do
    command -v "$tool" >/dev/null || { printf 'fetch-beir: FAIL — %s not found on PATH\n' "$tool" >&2; exit 1; }
done

datasets=("$@")
if [ ${#datasets[@]} -eq 0 ]; then
    datasets=(scifact nfcorpus fiqa)
fi
mkdir -p "$cache"

verify() { # path expected_bytes expected_sha
    local path="$1" want_bytes="$2" want_sha="$3" have_bytes have_sha
    have_bytes="$(wc -c < "$path" | tr -d ' ')"
    have_sha="$(shasum -a 256 "$path" | cut -d' ' -f1)"
    if [ "$have_bytes" != "$want_bytes" ] || [ "$have_sha" != "$want_sha" ]; then
        printf 'fetch-beir: FAIL — %s\n  expected %s bytes sha256=%s\n  actual   %s bytes sha256=%s\n' \
            "$path" "$want_bytes" "$want_sha" "$have_bytes" "$have_sha" >&2
        exit 1
    fi
}

for d in "${datasets[@]}"; do
    url="$(jq -er ".datasets[\"$d\"].url" "$manifest")" || { printf 'fetch-beir: FAIL — unknown dataset %s\n' "$d" >&2; exit 1; }
    zip="$cache/$d.zip"
    if [ ! -f "$zip" ]; then
        printf 'fetch-beir: downloading %s\n' "$url"
        # Retries with backoff cover transient connection failures (research R4 — seen on the
        # first CI run: `curl (7) Failed to connect … after 1594 ms`). A host that stays
        # unreachable still fails here, loudly and as a *download* failure — never as a hash
        # failure, and never by falling back to another source (spec FR-007).
        if ! curl -sSL --fail \
                --connect-timeout 20 --max-time 600 \
                --retry 5 --retry-delay 5 --retry-all-errors \
                -o "$zip.part" "$url"; then
            rm -f "$zip.part"
            printf 'fetch-beir: FAIL — could not download %s after 6 attempts (network/source, not a hash problem)\n' "$url" >&2
            exit 1
        fi
        mv "$zip.part" "$zip"
    else
        printf 'fetch-beir: %s already cached\n' "$d.zip"
    fi
    verify "$zip" "$(jq -r ".datasets[\"$d\"].archive.bytes" "$manifest")" "$(jq -r ".datasets[\"$d\"].archive.sha256" "$manifest")"
    (cd "$cache" && unzip -qo "$d.zip")
    while IFS=$'\t' read -r rel bytes sha; do
        verify "$cache/$d/$rel" "$bytes" "$sha"
    done < <(jq -r ".datasets[\"$d\"].files | to_entries[] | [.key, (.value.bytes|tostring), .value.sha256] | @tsv" "$manifest")
    printf 'fetch-beir: PASS — %s (archive + %s files verified)\n' "$d" "$(jq -r ".datasets[\"$d\"].files | length" "$manifest")"
done
