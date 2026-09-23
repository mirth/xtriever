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
    "RerankMode",  # Feature 015
    "RerankReport",
    "SearchOptions",
    "SearchResponse",
    "StageReport",
    "XtrieverError",
]
BUILDER_NAMES = ["Document", "FieldDef", "FieldKind", "FieldValue", "IndexConfig"]
SPARSE_NAMES = ["SparseInfo", "SparseOptionConfig"]  # Feature 027
EXPECTED_ALL = sorted(SEARCH_NAMES + BUILDER_NAMES + SPARSE_NAMES + ["__version__"])

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
    assert o.rerank_mode is None, "None = the index's recorded mode (Feature 015)"
    assert o.max_time_ms is None and o.max_items is None
    assert o.strict is False and o.explain is False


def test_load_paths():
    assert xtriever.LoadPath.MMAP is not xtriever.LoadPath.BUFFERED
    assert {p.name for p in xtriever.LoadPath} == {"BUFFERED", "MMAP"}


def test_builder_defaults():
    cfg = xtriever.IndexConfig(fields=[], dense_fields=["text"])
    assert (cfg.candidate_depth, cfg.rrf_k, cfg.rerank_depth) == (100, 60, 20)
    assert cfg.rerank_mode is None, "None = the engine's default at build (interpolate, alpha 0.5)"
    f = xtriever.FieldDef(name="text", kind=xtriever.FieldKind.TEXT(analyzer="standard"))
    assert (f.indexed, f.stored, f.boost) == (True, False, 1.0)
    d = xtriever.Document(external_id="x", fields={"text": xtriever.FieldValue.TEXT("t")})
    assert d.chunk is None


def test_hit_is_keyword_constructible():
    hit = xtriever.Hit(external_id="x", text="t", score=1.0, rerank_score=None, chunk=None, explain=None)
    assert hit.external_id == "x"
    with pytest.raises(TypeError):
        xtriever.Hit("x")  # positional construction is not part of the surface


def test_rerank_mode_surface():
    replace = xtriever.RerankMode.REPLACE()
    interp = xtriever.RerankMode.INTERPOLATE(alpha=0.5)
    assert isinstance(replace, xtriever.RerankMode.REPLACE)
    assert isinstance(interp, xtriever.RerankMode.INTERPOLATE)
    assert interp.alpha == 0.5
    assert xtriever.SearchOptions(k=1, rerank_mode=replace).rerank_mode == replace
    e = xtriever.HitExplain(bm25_score=None, bm25_rank=None, dense_score=None, dense_rank=None,
                            fused=0.0, rerank_score=None, rerank_rank=None, rerank_combined=None)
    assert e.rerank_combined is None
