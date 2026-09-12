#!/usr/bin/env bash
# Fail if the NotImplemented scaffold survived past the implementation PRs.
#
# `SpikeError::NotImplemented` exists so the acceptance tests could be committed failing (FR-013)
# with clean red *failures* rather than compile errors. It is temporary by construction, and
# ADR-0003's containment is worth nothing if scaffolding quietly becomes permanent.

set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"

# Features 002–005 use the same device: `xtriever-lexical`'s scaffold module returns
# `Error::backend(NotImplemented(..))` from every method until the real modules replace it (T060).
status=0
for crate in xtriever-ffi xtriever-lexical xtriever-eval xtriever-dense xtriever-pipeline; do
    if grep -rq 'NotImplemented' "$repo_root/crates/$crate/src/"; then
        printf 'check-no-stubs: FAIL — NotImplemented scaffolding still present in crates/%s/src/:\n' "$crate" >&2
        grep -rn 'NotImplemented' "$repo_root/crates/$crate/src/" | sed 's/^/  /' >&2
        status=1
    fi
done
if [ "$status" -ne 0 ]; then
    printf '  The operations are implemented; the scaffold must be removed.\n' >&2
    exit 1
fi
printf 'check-no-stubs: PASS — no scaffolding left in crates/{xtriever-ffi,xtriever-lexical,xtriever-eval,xtriever-dense,xtriever-pipeline}/src/\n'
