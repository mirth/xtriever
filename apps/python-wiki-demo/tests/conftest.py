"""Shared paths, skips and helpers for the demo's suite (Feature 019; the Feature 011 pattern).

Tests marked ``models`` need the two pinned engine models and the 007 fixture index on
disk (``scripts/fetch-model.sh``; ``cargo run --release -p xtriever-ffi --example fixture_index --
swift/Xtriever/Tests/Fixtures``); tests marked ``chonky`` need the demo's ``chonky`` extra
installed and its pinned model on disk (Feature 023: ``--chunker chonky`` is optional).
Without them they are skipped with the missing piece in the reason — never silently green. The Wikipedia artefact (``target/xt-wiki``) is optional: the
tests that can use it fall back to the fixture and say so.
"""

import importlib.util
import io
import json
import os
import struct
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3]
FIXTURE_INDEX = REPO / "swift/Xtriever/Tests/Fixtures/index"
FIXTURE_GOLDENS = REPO / "swift/Xtriever/Tests/Fixtures/expected.json"
EMBEDDER = Path(os.environ.get("XTRIEVER_MODEL_DIR", REPO / "reference/models/all-MiniLM-L6-v2-q8"))
RERANKER = Path(
    os.environ.get("XTRIEVER_RERANK_MODEL_DIR", REPO / "reference/models/ms-marco-MiniLM-L-6-v2-q8")
)
CHONKY = Path(
    os.environ.get("XTRIEVER_CHONKY_MODEL_DIR", REPO / "reference/models/chonky_distilbert_base_uncased_1")
)
WIKI_ARTEFACT = REPO / "target/xt-wiki"
CHUNK_A = REPO / "reference/fixtures/008/chunk_a.json"
CHUNK_B = REPO / "reference/fixtures/008/chunk_b.json"
QUERIES_008 = REPO / "reference/fixtures/008/queries.json"
MANIFEST_008 = REPO / "reference/datasets/wiki-manifest.json"


def missing_for_models():
    """The first prerequisite of the model-backed tests that is absent, or None."""
    for path in (
        EMBEDDER / "model.safetensors",
        RERANKER / "model.safetensors",
        FIXTURE_INDEX / "xtriever-pipeline.json",
        FIXTURE_GOLDENS,
    ):
        if not path.exists():
            return path
    return None


CHONKY_INSTALL = "uv pip install --python apps/python-wiki-demo/.venv/bin/python -e 'apps/python-wiki-demo[chonky]'"
CHONKY_FETCH = "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json"


def missing_for_chonky():
    """Why a ``chonky`` test cannot run: the extra is not installed, or its model is absent; or None."""
    if importlib.util.find_spec("chonky") is None:
        return f"the chonky extra is not installed: {CHONKY_INSTALL}"
    if not (CHONKY / "model.safetensors").exists():
        return f"chonky model not on disk: {CHONKY / 'model.safetensors'} ({CHONKY_FETCH})"
    return None


def pytest_collection_modifyitems(config, items):
    missing = missing_for_models()
    missing_chonky = missing_for_chonky()
    for item in items:
        if missing is not None and "models" in item.keywords:
            item.add_marker(pytest.mark.skip(reason=f"models/fixture not on disk: {missing}"))
        if missing_chonky is not None and "chonky" in item.keywords:
            item.add_marker(pytest.mark.skip(reason=missing_chonky))


def f64_bits(x):
    """The IEEE-754 bits of a Python float as 16 hex digits — the goldens' `score_bits`."""
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def f32_bits(x):
    """The f32 bits as 8 hex digits, or None — the goldens' `*_score_bits`."""
    if x is None:
        return None
    return "%08x" % struct.unpack("<I", struct.pack("<f", x))[0]


def run_cli(argv, env=None):
    """Run ``wikidemo.cli.main`` in-process; returns (exit code, stdout, stderr)."""
    from wikidemo.cli import main

    saved = {k: os.environ.get(k) for k in (env or {})}
    os.environ.update(env or {})
    out, err = io.StringIO(), io.StringIO()
    try:
        with redirect_stdout(out), redirect_stderr(err):
            try:
                code = main(list(argv))
            except SystemExit as e:  # argparse
                code = e.code if isinstance(e.code, int) else 2
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    return code, out.getvalue(), err.getvalue()


@pytest.fixture(scope="session")
def fixture_goldens():
    return json.loads(FIXTURE_GOLDENS.read_text(encoding="utf-8"))


@pytest.fixture(scope="session")
def fixture_handle():
    import xtriever

    return xtriever.IndexHandle.open(
        str(FIXTURE_INDEX), str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP
    )


@pytest.fixture(scope="session")
def fixture_artefact(tmp_path_factory):
    """An 008-shaped artefact directory over the 007 fixture index: ``index`` → the fixture,
    an ``ATTRIBUTION.txt`` stub, no ``corpus.json`` (the CLI must accept that)."""
    root = tmp_path_factory.mktemp("fixture-artefact")
    (root / "index").symlink_to(FIXTURE_INDEX, target_is_directory=True)
    (root / "ATTRIBUTION.txt").write_text("Fixture attribution (test stub).\n", encoding="utf-8")
    return root
