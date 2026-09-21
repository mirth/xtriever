"""Shared paths, skips and helpers for the xtriever Python suite (Feature 011).

Tests marked ``models`` need the two pinned models and the 007 fixture index on disk
(``scripts/fetch-model.sh``; ``cargo run --release -p xtriever-ffi --example fixture_index --
swift/Xtriever/Tests/Fixtures``). Without them they are skipped with the missing path in the
reason — never silently green. CI runs only the model-free subset (``-m "not models"``).
"""

import json
import os
import struct
from pathlib import Path

import pytest
from model_dirs import weights

REPO = Path(__file__).resolve().parents[2]
FIXTURE_INDEX = REPO / "swift/Xtriever/Tests/Fixtures/index"
EMBEDDER = Path(os.environ.get("XTRIEVER_MODEL_DIR", REPO / "reference/models/all-MiniLM-L6-v2-q8"))
RERANKER = Path(
    os.environ.get("XTRIEVER_RERANK_MODEL_DIR", REPO / "reference/models/ms-marco-MiniLM-L-6-v2-q8")
)
GOLDENS = REPO / "swift/Xtriever/Tests/Fixtures/expected.json"
FIXTURE_DOCS = REPO / "reference/fixtures/005/hybrid.json"


def missing_for_models():
    """The first prerequisite of the model-backed tests that is absent, or None."""
    for model_dir in (EMBEDDER, RERANKER):
        if weights(model_dir) is None:
            return model_dir / "{model.safetensors,*.gguf}"
    for path in (FIXTURE_INDEX / "xtriever-pipeline.json", GOLDENS):
        if not path.exists():
            return path
    return None


def pytest_collection_modifyitems(config, items):
    missing = missing_for_models()
    if missing is None:
        return
    skip = pytest.mark.skip(reason=f"models/fixture not on disk: {missing}")
    for item in items:
        if "models" in item.keywords:
            item.add_marker(skip)


def f64_bits(x):
    """The IEEE-754 bits of a Python float as 16 hex digits — the goldens' `score_bits`."""
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def f32_bits(x):
    """The f32 bits as 8 hex digits, or None — the goldens' `*_score_bits`."""
    if x is None:
        return None
    return "%08x" % struct.unpack("<I", struct.pack("<f", x))[0]


@pytest.fixture(scope="module")
def goldens():
    return json.loads(GOLDENS.read_text())


@pytest.fixture(scope="module")
def handle():
    """The fixture index with both models, memory-mapped."""
    import xtriever

    return xtriever.IndexHandle.open(
        str(FIXTURE_INDEX), str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP
    )


@pytest.fixture(scope="module")
def handle_fused():
    """The fixture index without a re-ranker: fused order only."""
    import xtriever

    return xtriever.IndexHandle.open(str(FIXTURE_INDEX), str(EMBEDDER), None, xtriever.LoadPath.MMAP)


def golden_hit_tuples(hits):
    """A golden's hits in the shape `search_hit_tuples` produces."""
    return [
        (
            h["external_id"],
            h["score_bits"],
            h.get("rerank_score_bits"),
            h.get("bm25_score_bits"),
            h.get("dense_score_bits"),
            h.get("rerank_rank"),
        )
        for h in hits
    ]


def search_hit_tuples(hits):
    """A response's hits as `(id, fused bits, re-rank bits, bm25 bits, dense bits, re-rank rank)`."""
    return [
        (
            h.external_id,
            f64_bits(h.score),
            f32_bits(h.rerank_score),
            f32_bits(h.explain.bm25_score),
            f32_bits(h.explain.dense_score),
            h.explain.rerank_rank,
        )
        for h in hits
    ]
