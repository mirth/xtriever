#!/usr/bin/env bash
# Syntax-aware containment checks for the constitution's hard constraints (Principles III, VII).
#
# The naive `grep -rn unsafe` matches documentation as well as code, so it cannot be read as a
# gate (Feature 006 review round 1, comment 4). This script strips `//` comments to end of line
# and single-line `/* */` blocks before matching, and asserts exact counts:
#   - xtriever-dense and xtriever-rerank: exactly one `#[allow(unsafe_code)]` and one `unsafe {`
#     each, both in src/bytes.rs (ADR-0007, ADR-0009). The workspace lint `unsafe_code = "deny"`
#     guarantees no block exists outside an `allow` item, so counting the attribute is exact.
#   - the pure crates (core, analysis, pipeline, ltr, eval): zero `unsafe`, zero
#     `Instant`/`SystemTime`/`std::thread` in library code (tests may read a clock).
# Exit 1 on any deviation, printing the offending lines.

set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
status=0

code_lines() { # dir pattern -> "file:line:code" for code (non-comment) lines matching pattern
    grep -rn --include='*.rs' -E "$2" "$1" \
        | sed -E 's|/\*[^*]*\*/||g' \
        | awk -F: '{ line=$0; sub(/^[^:]+:[0-9]+:/, "", line); sub(/\/\/.*$/, "", line); if (line ~ /'"$2"'/) print $0 }'
}

expect_count() { # label dir pattern expected
    local hits n
    hits="$(code_lines "$2" "$3" || true)"
    n="$(printf '%s' "$hits" | grep -c . || true)"
    if [ "$n" -ne "$4" ]; then
        printf 'check-containment: FAIL — %s: expected %s code line(s) matching /%s/, found %s\n' "$1" "$4" "$3" "$n" >&2
        printf '%s\n' "$hits" | sed 's/^/  /' >&2
        status=1
    else
        printf 'ok  %-58s %s line(s) match /%s/\n' "$1" "$n" "$3"
    fi
}

for crate in xtriever-dense xtriever-rerank; do
    expect_count "$crate allow(unsafe_code)" "$repo_root/crates/$crate/src" '#\[allow\(unsafe_code\)\]' 1
    expect_count "$crate unsafe blocks" "$repo_root/crates/$crate/src" '(^|[^[:alnum:]_])unsafe \{' 1
    if [ "$(code_lines "$repo_root/crates/$crate/src" '(^|[^[:alnum:]_])unsafe($|[^[:alnum:]_])' | grep -v '/src/bytes\.rs:' | grep -c . || true)" -ne 0 ]; then
        printf 'check-containment: FAIL — %s: unsafe outside src/bytes.rs\n' "$crate" >&2
        status=1
    fi
done
for crate in xtriever-core xtriever-analysis xtriever-pipeline xtriever-ltr xtriever-eval; do
    expect_count "$crate unsafe" "$repo_root/crates/$crate/src" '(^|[^[:alnum:]_])unsafe($|[^[:alnum:]_])' 0
    expect_count "$crate clock/threads" "$repo_root/crates/$crate/src" 'Instant|SystemTime|std::thread' 0
done

if [ "$status" -ne 0 ]; then exit 1; fi
printf 'check-containment: PASS\n'
