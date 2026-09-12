#!/usr/bin/env bash
# Fetch and verify the pinned embedding model for xtriever-dense (Feature 004, spec FR-026).
#
# Usage: scripts/fetch-model.sh [DEST_DIR]      (default: reference/models/all-MiniLM-L6-v2)
#
# The three files (config.json, tokenizer.json, model.safetensors) are downloaded from the
# Hugging Face hub at the revision pinned in reference/models/manifest.json and checked against
# the manifest — size AND sha256. A mismatch prints the path and both values and exits 1; a
# download failure is reported as such and never as a hash failure. Idempotent: a file already
# present is not downloaded again, but everything is re-verified. The destination is git-ignored.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$repo_root/reference/models/manifest.json"
dest="${1:-$repo_root/reference/models/all-MiniLM-L6-v2}"

for tool in curl jq shasum; do
    command -v "$tool" >/dev/null || { printf 'fetch-model: FAIL — %s not found on PATH\n' "$tool" >&2; exit 1; }
done

repository="$(jq -er '.repository' "$manifest")"
revision="$(jq -er '.revision' "$manifest")"
mkdir -p "$dest"

file_size() { # portable stat
    if stat -f %z "$1" >/dev/null 2>&1; then stat -f %z "$1"; else stat -c %s "$1"; fi
}

verify() { # path expected_bytes expected_sha
    local path="$1" want_bytes="$2" want_sha="$3" have_bytes have_sha
    have_bytes="$(file_size "$path")"
    have_sha="$(shasum -a 256 "$path" | cut -d' ' -f1)"
    if [ "$have_bytes" != "$want_bytes" ] || [ "$have_sha" != "$want_sha" ]; then
        printf 'fetch-model: FAIL — %s does not match the pin\n  expected %s bytes sha256=%s\n  actual   %s bytes sha256=%s\n' \
            "$path" "$want_bytes" "$want_sha" "$have_bytes" "$have_sha" >&2
        exit 1
    fi
    printf 'verified %s %s %s\n' "$(basename "$path")" "$have_bytes" "$have_sha"
}

n="$(jq -r '.files | length' "$manifest")"
for ((i = 0; i < n; i++)); do
    name="$(jq -er ".files[$i].name" "$manifest")"
    want_bytes="$(jq -er ".files[$i].bytes" "$manifest")"
    want_sha="$(jq -er ".files[$i].sha256" "$manifest")"
    path="$dest/$name"
    if [ ! -f "$path" ]; then
        url="https://huggingface.co/$repository/resolve/$revision/$name"
        printf 'fetch-model: downloading %s\n' "$url"
        if ! curl -sSL --retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 20 \
                -o "$path.part" "$url"; then
            rm -f "$path.part"
            printf 'fetch-model: FAIL — download failed for %s (network/source problem, not a hash mismatch)\n' "$url" >&2
            exit 1
        fi
        mv "$path.part" "$path"
    fi
    verify "$path" "$want_bytes" "$want_sha"
done
printf 'fetch-model: PASS — %s at %s verified in %s\n' "$repository" "$revision" "$dest"
