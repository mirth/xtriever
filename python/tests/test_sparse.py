"""Feature 027 (User Story 4): a sparse index built and searched from Python. Built with the
option, it reports it (format version 3, the scale, the boost, the encoder); opened again with
no encoder, it answers exactly as the handle that built it. Needs the two pinned models and the
sparse encoder (``scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json``)."""

import pytest

import xtriever
from conftest import EMBEDDER, RERANKER, SPARSE_ENCODER, search_hit_tuples
from test_build import build, config, fixture

pytestmark = [
    pytest.mark.models,
    pytest.mark.skipif(
        not (SPARSE_ENCODER / "model.safetensors").exists(),
        reason=f"sparse encoder absent: {SPARSE_ENCODER}",
    ),
]


def build_sparse(tmp_path, h):
    cfg = config(h)
    cfg.sparse = xtriever.SparseOptionConfig(encoder_dir=str(SPARSE_ENCODER))
    return build(tmp_path, h, cfg=cfg)


def test_a_python_built_sparse_index_reports_the_option(tmp_path):
    info = build_sparse(tmp_path, fixture()).info()
    assert info.format_version == 3
    assert info.sparse is not None
    assert (info.sparse.scale, info.sparse.boost) == (10, 1.0)
    assert info.sparse.encoder.startswith("opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill@")


def test_a_reopened_sparse_index_answers_as_the_one_that_built_it(tmp_path):
    h = fixture()
    built = build_sparse(tmp_path, h)
    reopened = xtriever.IndexHandle.open(str(tmp_path / "idx"), str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP)
    plain = build(tmp_path, h, name="plain")
    assert built.info().sparse is not None, "the index was not built sparse"
    assert reopened.info().sparse == built.info().sparse
    # Strict: a query whose expansion could not be built is an error here, not a quiet fallback
    # to the text fields that both handles would share.
    opts = xtriever.SearchOptions(k=10, explain=True, strict=True)
    differs = 0
    for q in h["queries"]:
        a = built.search(q["text"], opts)
        b = reopened.search(q["text"], opts)
        assert search_hit_tuples(a.hits) == search_hit_tuples(b.hits), q["id"]
        differs += search_hit_tuples(a.hits) != search_hit_tuples(plain.search(q["text"], opts).hits)
    # The same documents without the option must answer differently somewhere, or the
    # expansion did nothing.
    assert differs > 0, "the expansion changed no fixture query's results"
