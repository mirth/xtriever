"""Model-free: the package's surface is the engine's exports, nothing more (contract §1, §2)."""

import re

import pytest

import xtriever
from conftest import REPO

SEARCH_NAMES = [
    "ChunkInfo",
    "Degradation",
    "DegradeReason",
    "Hit",
    "HitExplain",
    "IndexHandle",
    "IndexInfo",
    "LoadPath",
    "RerankReport",
    "SearchOptions",
    "SearchResponse",
    "StageReport",
    "XtrieverError",
]
# PR B adds the builder (Feature 011 US4): IndexConfig, FieldDef, FieldKind, FieldValue, Document.
EXPECTED_ALL = sorted(SEARCH_NAMES + ["__version__"])

ERROR_KINDS = [
    "Schema",
    "InvalidQuery",
    "UnknownField",
    "DimensionMismatch",
    "NotFound",
    "Model",
    "Corrupt",
    "FingerprintMismatch",
    "BudgetExhausted",
    "Io",
    "Backend",
]


def test_version_is_the_workspace_crate_version():
    cargo = (REPO / "Cargo.toml").read_text()
    m = re.search(r'^\[workspace\.package\](?:.*\n)*?version = "([^"]+)"', cargo, re.M)
    assert m, "workspace.package.version not found"
    assert xtriever.__version__ == m.group(1)


def test_all_is_exactly_the_contract():
    assert sorted(xtriever.__all__) == EXPECTED_ALL
    for name in xtriever.__all__:
        assert hasattr(xtriever, name), name


def test_every_error_kind_is_a_subclass_of_the_base():
    assert issubclass(xtriever.XtrieverError, Exception)
    for kind in ERROR_KINDS:
        cls = getattr(xtriever.XtrieverError, kind)
        assert issubclass(cls, xtriever.XtrieverError), kind


def test_search_options_defaults():
    o = xtriever.SearchOptions(k=10)
    assert o.k == 10
    assert o.depth is None and o.rerank_depth is None
    assert o.max_time_ms is None and o.max_items is None
    assert o.strict is False and o.explain is False


def test_load_paths():
    assert xtriever.LoadPath.MMAP is not xtriever.LoadPath.BUFFERED
    assert {p.name for p in xtriever.LoadPath} == {"BUFFERED", "MMAP"}


def test_hit_is_keyword_constructible():
    hit = xtriever.Hit(external_id="x", text="t", score=1.0, rerank_score=None, chunk=None, explain=None)
    assert hit.external_id == "x"
    with pytest.raises(TypeError):
        xtriever.Hit("x")  # positional construction is not part of the surface
