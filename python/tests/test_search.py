"""The Python results are the engine's, bit for bit (spec US1/US2, SC-002): every 007 golden
query, with and without the re-ranker, compared on ids, order, fused f64 bits, re-rank f32
bits, the bm25/dense feature bits and the re-rank rank.

Green at the red commit by construction: the surface is Feature 007's. The tests exist so
that this feature — and any later one — cannot change what Python sees."""

import pytest

import xtriever
from conftest import golden_hit_tuples, search_hit_tuples

pytestmark = pytest.mark.models


def _run(handle, q):
    return handle.search(
        q["text"], xtriever.SearchOptions(k=q["k"], rerank_depth=q["rerank_depth"], explain=True)
    )


def test_every_golden_pair_is_bit_identical(goldens, handle, handle_fused):
    pairs = 0
    for q in goldens["queries"]:
        for h, key in ((handle_fused, "without_reranker"), (handle, "with_reranker")):
            r = _run(h, q)
            assert search_hit_tuples(r.hits) == golden_hit_tuples(q[key]["hits"]), (q["id"], key)
            stages = q[key]["stages"]
            assert r.stages.lexical_candidates == stages["lexical_candidates"]
            assert r.stages.dense_candidates == stages["dense_candidates"]
            assert (r.stages.degraded is None) == (stages["degraded"] is False)
            if "rerank" in stages and stages["rerank"] is not None:
                assert r.stages.rerank is not None
                assert r.stages.rerank.candidates == stages["rerank"]["candidates"]
                assert r.stages.rerank.scored == stages["rerank"]["scored"]
            pairs += 1
    assert pairs == 16


def test_hits_carry_text_and_eight_features(goldens, handle):
    q = goldens["queries"][0]
    r = _run(handle, q)
    assert r.hits, "the first query has hits"
    for hit in r.hits:
        assert hit.text
        e = hit.explain
        assert e is not None
        assert isinstance(e.fused, float)
        # A hit not seen by a stage carries None for that stage's score and rank.
        assert (e.bm25_score is None) == (e.bm25_rank is None)
        assert (e.dense_score is None) == (e.dense_rank is None)
        assert (e.rerank_score is None) == (e.rerank_rank is None)
        # Feature 015: the combined score exists exactly where the interpolating rule ordered.
        assert (e.rerank_combined is None) == (e.rerank_score is None)
    assert r.elapsed_ms >= 0


def test_info_is_the_index_identity(goldens, handle, handle_fused):
    info = handle.info()
    assert info.documents == 40
    assert info.format_version == 2
    assert info.embedder_fingerprint == goldens["info"]["embedder_fingerprint"]
    assert info.reranker_model_id is not None
    assert info.reranker_load_ms is not None
    assert info.candidate_depth == goldens["info"]["candidate_depth"]
    assert info.rrf_k == goldens["info"]["rrf_k"]
    fused = handle_fused.info()
    assert fused.reranker_model_id is None and fused.reranker_load_ms is None
