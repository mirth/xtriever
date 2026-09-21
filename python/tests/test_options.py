"""The engine's option semantics, reachable from Python (spec FR-007): degradation vs strict,
depths, explain, k = 0, long and non-ASCII queries."""

import pytest

import xtriever

pytestmark = pytest.mark.models


def test_non_strict_budget_degrades_strict_raises(handle):
    r = handle.search("lantern", xtriever.SearchOptions(k=5, max_time_ms=1))
    assert isinstance(r.hits, list)
    degraded = r.stages.degraded
    skipped = r.stages.rerank.skipped if r.stages.rerank is not None else None
    assert degraded is not None or skipped is not None, "a 1 ms budget cuts a stage"
    reason = degraded.reason if degraded is not None else skipped
    assert isinstance(reason, xtriever.DegradeReason.BUDGET_EXCEEDED)
    with pytest.raises(xtriever.XtrieverError.BudgetExhausted):
        handle.search("lantern", xtriever.SearchOptions(k=5, max_time_ms=1, strict=True))


def test_rerank_depth_zero_scores_nothing(handle):
    r = handle.search("lantern", xtriever.SearchOptions(k=5, rerank_depth=0))
    assert r.stages.rerank is None or r.stages.rerank.scored == 0
    assert all(h.rerank_score is None for h in r.hits)


def test_explain_off_leaves_no_explanation(handle):
    r = handle.search("lantern", xtriever.SearchOptions(k=5))
    assert r.hits and all(h.explain is None for h in r.hits)


def test_k_zero_is_empty(handle):
    r = handle.search("lantern", xtriever.SearchOptions(k=0))
    assert r.hits == []


def test_depth_bounds_the_candidates(handle):
    r = handle.search("lantern", xtriever.SearchOptions(k=5, depth=1))
    assert r.stages.lexical_candidates <= 1
    assert r.stages.dense_candidates is None or r.stages.dense_candidates <= 1


def test_long_and_non_ascii_queries_are_accepted(handle):
    long_query = " ".join(["lantern"] * 2000)
    r = handle.search(long_query, xtriever.SearchOptions(k=3))
    assert isinstance(r.hits, list)
    r = handle.search("café lanterne 灯笼", xtriever.SearchOptions(k=3))
    assert isinstance(r.hits, list)


# The pre-015 re-ranked order of the first golden query ("zephyr", k 10, depth 5) on `main`.
# The eight-bit embedder's fused tail (Feature 026) — crates/xtriever-ffi/tests/options.rs pins the same.
Q0_REPLACE_ORDER = ["d016", "d011", "d031", "d026", "d001", "d032", "d020", "d038", "d008", "d030"]


def test_rerank_mode_override_and_info(handle):
    info = handle.info()
    assert info.rerank_mode == xtriever.RerankMode.INTERPOLATE(alpha=0.5), "the default, recorded"
    replaced = handle.search(
        "zephyr", xtriever.SearchOptions(k=10, rerank_depth=5, explain=True, rerank_mode=xtriever.RerankMode.REPLACE())
    )
    assert [h.external_id for h in replaced.hits] == Q0_REPLACE_ORDER
    assert all(h.explain.rerank_combined is None for h in replaced.hits)
    interpolated = handle.search("zephyr", xtriever.SearchOptions(k=10, rerank_depth=5, explain=True))
    assert sum(h.explain.rerank_combined is not None for h in interpolated.hits) == 5
    assert [h.external_id for h in interpolated.hits[5:]] == Q0_REPLACE_ORDER[5:], "the tail is the fused order"
    for h in interpolated.hits[:5]:
        assert 0.0 <= h.explain.rerank_combined <= 1.0
    with pytest.raises(xtriever.XtrieverError.Schema):
        handle.search("zephyr", xtriever.SearchOptions(k=5, rerank_mode=xtriever.RerankMode.INTERPOLATE(alpha=2.0)))
