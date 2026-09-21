#!/usr/bin/env bash
# Fetch and verify a pinned model (Feature 004 spec FR-026; Feature 006 spec FR-003).
#
# Usage: scripts/fetch-model.sh [--manifest FILE] [DEST_DIR]
#   default manifest: reference/models/manifest.json (the 004 embedder)
#   default DEST_DIR: reference/models/<local_dir from the manifest>, else
#                     reference/models/all-MiniLM-L6-v2
#   e.g. scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json   (006 cross-encoder)
#
# The three files (config.json, tokenizer.json, model.safetensors) are downloaded from the
# Hugging Face hub at the revision pinned in the manifest and checked against
# the manifest — size AND sha256. A mismatch prints the path and both values and exits 1; a
# download failure is reported as such and never as a hash failure. Idempotent: a file already
# present is not downloaded again, but everything is re-verified. The destination is git-ignored.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$repo_root/reference/models/manifest.json"
if [ "${1:-}" = "--manifest" ]; then
    [ -n "${2:-}" ] || { printf 'fetch-model: FAIL — --manifest needs a file\n' >&2; exit 1; }
    manifest="$2"
    shift 2
fi
[ -f "$manifest" ] || { printf 'fetch-model: FAIL — manifest %s does not exist\n' "$manifest" >&2; exit 1; }

for tool in curl jq shasum; do
    command -v "$tool" >/dev/null || { printf 'fetch-model: FAIL — %s not found on PATH\n' "$tool" >&2; exit 1; }
done

repository="$(jq -er '.repository' "$manifest")"
revision="$(jq -er '.revision' "$manifest")"
local_dir="$(jq -r '.local_dir // "all-MiniLM-L6-v2"' "$manifest")"
dest="${1:-$repo_root/reference/models/$local_dir}"
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
# Feature 026: an eight-bit artefact supplies weights only. Its manifest's `borrows` names the
# float manifest whose `config.json` and `tokenizer.json` sit beside the weights, fetched (if
# absent) and verified against *that* manifest's pins, then copied here and verified again.
if [ "$(jq -r '.borrows // empty' "$manifest")" != "" ]; then
    borrowed_manifest="$(dirname "$manifest")/$(jq -er '.borrows.manifest' "$manifest")"
    borrowed_dir="$repo_root/reference/models/$(jq -r '.local_dir // "all-MiniLM-L6-v2"' "$borrowed_manifest")"
    "$0" --manifest "$borrowed_manifest" >/dev/null
    m="$(jq -r '.borrows.files | length' "$manifest")"
    for ((j = 0; j < m; j++)); do
        name="$(jq -er ".borrows.files[$j]" "$manifest")"
        want_bytes="$(jq -er --arg n "$name" '.files[] | select(.name == $n) | .bytes' "$borrowed_manifest")"
        want_sha="$(jq -er --arg n "$name" '.files[] | select(.name == $n) | .sha256' "$borrowed_manifest")"
        cp "$borrowed_dir/$name" "$dest/$name"
        verify "$dest/$name" "$want_bytes" "$want_sha"
    done
    printf 'fetch-model: borrowed %s from %s\n' "$(jq -r '.borrows.files | join(", ")' "$manifest")" "$borrowed_dir"
    # Tensors the artefact lacks, copied byte for byte out of the borrowed float weights into a
    # small safetensors file (deterministic, so its pin is checkable): the re-ranker's pooler.
    if [ "$(jq -r '.borrows.tensors // empty' "$manifest")" != "" ]; then
        command -v python3 >/dev/null || { printf 'fetch-model: FAIL — python3 not found on PATH (needed for .borrows.tensors)\n' >&2; exit 1; }
        tfile="$(jq -er '.borrows.tensors.file' "$manifest")"
        tfrom="$(jq -er '.borrows.tensors.from' "$manifest")"
        names="$(jq -r '.borrows.tensors.names | join(" ")' "$manifest")"
        # shellcheck disable=SC2086
        python3 "$repo_root/scripts/extract_tensors.py" "$borrowed_dir/$tfrom" "$dest/$tfile" $names >/dev/null
        verify "$dest/$tfile" "$(jq -er '.borrows.tensors.bytes' "$manifest")" "$(jq -er '.borrows.tensors.sha256' "$manifest")"
    fi
fi
printf 'fetch-model: PASS — %s at %s verified in %s\n' "$repository" "$revision" "$dest"
