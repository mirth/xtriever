#!/usr/bin/env bash
# Create the pinned virtualenv that generates Feature 001's golden fixtures.
#
# Why a pinned interpreter: the system python3 here is 3.14, and torch publishes no wheel for
# it — so `pip install torch` fails with a message that looks like a broken environment rather
# than an unsupported interpreter. The system interpreter is also PEP 668 externally-managed.
#
# Why pinned + hashed requirements: these fixtures ARE the oracle. A different torch build
# silently produces a different golden embedding, and every downstream comparison inherits it.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
py_version="$(tr -d '[:space:]' < "$repo_root/reference/.python-version")"
venv="$repo_root/reference/.venv-001"
req="$repo_root/reference/requirements-001.txt"

interpreter="$(command -v "python$py_version" || true)"
if [ -z "$interpreter" ]; then
    printf 'setup-reference-venv: FAIL — python%s not found on PATH\n' "$py_version" >&2
    printf '  fix: brew install python@%s\n' "$py_version" >&2
    exit 1
fi

printf 'setup-reference-venv: using %s (%s)\n' "$interpreter" "$("$interpreter" -V)"

if command -v uv >/dev/null 2>&1; then
    uv venv --python "$interpreter" "$venv"
    VIRTUAL_ENV="$venv" uv pip install --require-hashes --requirement "$req"
else
    "$interpreter" -m venv "$venv"
    "$venv/bin/pip" install --require-hashes --requirement "$req"
fi

printf 'setup-reference-venv: PASS — %s\n' "$venv"
printf '  run: %s/bin/python reference/gen_001_fixtures.py --seed 1\n' "$venv"
