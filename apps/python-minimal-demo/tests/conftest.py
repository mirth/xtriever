"""Paths, the `models` skip and a loader for the demo file (Feature 020; the 011/019 pattern).

The demo is one script, not a package: the tests load it by path. Tests marked ``models``
need the two pinned models (``scripts/fetch-model.sh --manifest reference/models/manifest-q8.json``
and ``--manifest reference/models/manifest-rerank-q8.json``); without them they skip with the
missing path in the reason — never silently green.
"""

import importlib.util
import os
import struct
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3]
DEMO = REPO / "apps/python-minimal-demo/demo.py"
EMBEDDER = Path(os.environ.get("XTRIEVER_MODEL_DIR", REPO / "reference/models/all-MiniLM-L6-v2-q8"))
RERANKER = Path(os.environ.get("XTRIEVER_RERANK_MODEL_DIR", REPO / "reference/models/ms-marco-MiniLM-L-6-v2-q8"))


def _load_by_path(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The engine suite's rule for which weights file a model directory holds (Feature 026), loaded
# by path like the demo itself: the demo depends on nothing but the wheel.
_model_dirs = _load_by_path("model_dirs", REPO / "python/tests/model_dirs.py")
weights, unusable_weights = _model_dirs.weights, _model_dirs.unusable_weights


def pytest_configure(config):
    config.addinivalue_line("markers", "models: needs the two pinned models on disk (skipped otherwise)")


def pytest_collection_modifyitems(config, items):
    for model_dir in (EMBEDDER, RERANKER):
        why = unusable_weights(model_dir)
        if why is None and weights(model_dir) is None:
            why = f"model not on disk: {model_dir}/{{model.safetensors,*.gguf}}"
        if why is not None:
            skip = pytest.mark.skip(reason=why)
            for item in items:
                if "models" in item.keywords:
                    item.add_marker(skip)
            return


def load_demo():
    """The demo module, loaded from its file."""
    return _load_by_path("demo", DEMO)


def f64_bits(x):
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def f32_bits(x):
    if x is None:
        return None
    return "%08x" % struct.unpack("<I", struct.pack("<f", x))[0]
