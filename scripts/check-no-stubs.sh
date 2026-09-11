#!/usr/bin/env bash
# Fail if the NotImplemented scaffold survived past the implementation PRs.
#
# `SpikeError::NotImplemented` exists so the acceptance tests could be committed failing (FR-013)
# with clean red *failures* rather than compile errors. It is temporary by construction, and
# ADR-0003's containment is worth nothing if scaffolding quietly becomes permanent.

set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"

if grep -rq 'NotImplemented' "$repo_root/crates/xtriever-ffi/src/"; then
    printf 'check-no-stubs: FAIL — SpikeError::NotImplemented still present:\n' >&2
    grep -rn 'NotImplemented' "$repo_root/crates/xtriever-ffi/src/" | sed 's/^/  /' >&2
    printf '  The three operations are implemented; the scaffold variant must be removed.\n' >&2
    exit 1
fi
printf 'check-no-stubs: PASS — no scaffolding left in crates/xtriever-ffi/src/\n'
