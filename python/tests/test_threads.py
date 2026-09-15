"""The interpreter lock is released while the engine works (spec FR-005, SC-005), and one
handle serialises concurrent searches without error."""

import threading
import time

import pytest

import xtriever

pytestmark = pytest.mark.models


def test_a_python_thread_runs_during_a_search(handle):
    handle.search("warm up", xtriever.SearchOptions(k=10, rerank_depth=20))
    timings = {}

    def work():
        t0 = time.perf_counter()
        for _ in range(20):
            sum(range(100_000))
        timings["thread"] = time.perf_counter() - t0

    t = threading.Thread(target=work)
    t0 = time.perf_counter()
    t.start()
    r = handle.search("what is the capital", xtriever.SearchOptions(k=10, rerank_depth=20))
    search = time.perf_counter() - t0
    t.join()
    assert r.hits
    assert "thread" in timings
    assert timings["thread"] < 0.25 * search, (timings["thread"], search)


def test_two_threads_search_one_handle(handle):
    errors = []
    counts = []

    def worker():
        try:
            for _ in range(5):
                r = handle.search("lantern harbour", xtriever.SearchOptions(k=10))
                counts.append(len(r.hits))
        except Exception as e:  # noqa: BLE001 — recorded, asserted below
            errors.append(e)

    threads = [threading.Thread(target=worker) for _ in range(2)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert errors == []
    assert counts == [10] * 10
