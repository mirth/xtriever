"""The demo's search prints the engine's values (spec SC-002): the fused call equals the 007
goldens' ``without_reranker`` lists and the re-ranked call their ``with_reranker`` lists, on
ids and score bits; the marks are computed between the two; exit codes per the contract."""

from types import SimpleNamespace

import pytest
import xtriever

from conftest import EMBEDDER, RERANKER, f32_bits, f64_bits, run_cli
from wikidemo.hits import marks
from wikidemo.inputs import resolve
from wikidemo.search import mode_label, open_artefact, run_search

pytestmark = pytest.mark.models


def _paths(fixture_artefact):
    return resolve(SimpleNamespace(artefact=str(fixture_artefact), embedder=str(EMBEDDER), reranker=str(RERANKER), snapshot=None, manifest=None, expected=None, queries=None))


@pytest.fixture(scope="module")
def opened(fixture_artefact):
    return open_artefact(_paths(fixture_artefact))


def _golden_tuples(hits):
    return [(h["external_id"], h["score_bits"], h["bm25_score_bits"], h["dense_score_bits"], h["rerank_score_bits"], h["rerank_rank"], h["rerank_combined_bits"]) for h in hits]


def _hit_tuples(hits):
    return [
        (
            h.external_id,
            f64_bits(h.score),
            f32_bits(h.explain.bm25_score),
            f32_bits(h.explain.dense_score),
            f32_bits(h.rerank_score),
            h.explain.rerank_rank,
            None if h.explain.rerank_combined is None else f64_bits(h.explain.rerank_combined),
        )
        for h in hits
    ]


def test_open_reports_timings(opened):
    assert opened.info.documents == 40
    assert opened.open_ms >= 0
    assert opened.info.embedder_load_ms >= 0 and opened.info.reranker_load_ms is not None
    assert mode_label(opened.info, "interpolate") == "interpolate α 0.5"
    assert mode_label(opened.info, "replace") == "replace"


def test_fused_list_equals_the_goldens_depth_zero(opened, fixture_goldens):
    for q in fixture_goldens["queries"]:
        [fused] = run_search(opened, q["text"], k=q["k"], depth=0)
        assert fused.label == "fused"
        assert fused.options.rerank_depth == 0
        assert _hit_tuples(fused.response.hits) == _golden_tuples(q["without_reranker"]["hits"]), q["id"]
        assert fused.wall_ms >= 0 and fused.peak_bytes > 0


def test_reranked_list_equals_the_goldens(opened, fixture_goldens):
    for q in fixture_goldens["queries"]:
        fused, reranked = run_search(opened, q["text"], k=q["k"], depth=q["rerank_depth"])
        assert reranked.label == "re-ranked"
        assert _hit_tuples(fused.response.hits) == _golden_tuples(q["without_reranker"]["hits"]), q["id"]
        assert _hit_tuples(reranked.response.hits) == _golden_tuples(q["with_reranker"]["hits"]), q["id"]
        rr = reranked.response.stages.rerank
        assert rr is not None and rr.candidates == q["with_reranker"]["stages"]["rerank"]["candidates"]


def test_marks_are_computed_between_the_two_calls(opened, fixture_goldens):
    q = fixture_goldens["queries"][0]
    fused, reranked = run_search(opened, q["text"], k=q["k"], depth=q["rerank_depth"])
    got, dropped = marks([h.external_id for h in fused.response.hits], [h.external_id for h in reranked.response.hits])
    assert set(got) == {h.external_id for h in reranked.response.hits}
    assert {d for d, _ in dropped} <= {h.external_id for h in fused.response.hits}


def test_depth_zero_is_one_call(opened):
    runs = run_search(opened, "zephyr", k=3, depth=0)
    assert len(runs) == 1 and runs[0].response.stages.rerank is None


def test_replace_mode_is_passed_through(opened, fixture_goldens):
    q = fixture_goldens["queries"][0]
    _, reranked = run_search(opened, q["text"], k=q["k"], depth=q["rerank_depth"], mode="replace")
    assert reranked.options.rerank_mode is not None and reranked.options.rerank_mode.is_REPLACE()
    assert all(h.explain.rerank_combined is None for h in reranked.response.hits)


def test_cli_prints_the_engine_values(fixture_artefact, fixture_goldens):
    q = fixture_goldens["queries"][0]
    env = {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)}
    code, out, err = run_cli(["search", "--artefact", str(fixture_artefact), "--explain", "-k", str(q["k"]), "--depth", "5", q["text"]], env)
    assert code == 0, err
    assert out.startswith("opened ")
    assert "warm-up: the first search of a process pages the vectors in" in out
    assert f"fused (lexical + dense), {len(q['without_reranker']['hits'])} hits, " in out
    assert "re-ranked (interpolate α 0.5, depth 5), " in out
    for h in q["with_reranker"]["hits"]:
        assert f"  {h['external_id']}" in out
    # The engine's fused score, printed with four decimals, for the top golden hit.
    top = q["with_reranker"]["hits"][0]
    import struct

    fused_value = struct.unpack("<d", struct.pack("<Q", int(top["score_bits"], 16)))[0]
    assert f"fused.score: {fused_value:.4f}" in out
    assert "stages: lexical " in out and "wall: fused " in out and "peak resident " in out


def test_strict_budget_error_exits_1(fixture_artefact, opened):
    env = {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)}
    code, out, err = run_cli(["search", "--artefact", str(fixture_artefact), "--budget-ms", "0", "--strict", "zephyr"], env)
    # What the engine raises for a zero budget in strict mode is the engine's decision; the
    # demo prints that class name and exits 1.
    try:
        opened.handle.search("zephyr", xtriever.SearchOptions(k=10, rerank_depth=0, max_time_ms=0, strict=True))
    except xtriever.XtrieverError as e:
        kind = type(e).__name__
    else:
        pytest.skip("the engine completed a zero-budget strict search on this machine")
    assert code == 1, out
    assert err.startswith(f"wikidemo: {kind}: ")


def test_no_hits_exits_0(fixture_artefact, opened):
    query = None
    for candidate in ("the of and", "qzxv", "of the a an"):
        [fused] = run_search(opened, candidate, k=10, depth=0)
        if not fused.response.hits:
            query = candidate
            break
    if query is None:
        pytest.skip("every candidate query has hits on the fixture")
    env = {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)}
    code, out, err = run_cli(["search", "--artefact", str(fixture_artefact), query], env)
    assert code == 0 and f'no passages found for "{query}"' in out
