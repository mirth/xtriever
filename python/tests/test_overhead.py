"""The binding adds a call, not a computation (spec SC-004): the wall time of `search` minus
the engine's own `elapsed_ms` is at most 5 % of `elapsed_ms`, as a median over the golden
queries."""

import statistics
import time

import pytest

import xtriever

pytestmark = pytest.mark.models


def test_binding_overhead_is_within_five_percent(goldens, handle):
    handle.search("warm up", xtriever.SearchOptions(k=10, rerank_depth=20))
    ratios = []
    for _ in range(3):
        for q in goldens["queries"]:
            opts = xtriever.SearchOptions(k=q["k"], rerank_depth=q["rerank_depth"], explain=True)
            t0 = time.perf_counter_ns()
            r = handle.search(q["text"], opts)
            wall_ms = (time.perf_counter_ns() - t0) / 1e6
            assert r.elapsed_ms > 0
            ratios.append((wall_ms - r.elapsed_ms) / r.elapsed_ms)
    median = statistics.median(ratios)
    print(f"binding overhead ratios: median {median:.4f}, max {max(ratios):.4f}, n {len(ratios)}")
    assert median <= 0.05, median
