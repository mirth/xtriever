"""The interpreter lock is released while the engine works (spec FR-005, SC-005), and one
handle serialises concurrent searches without error."""

import threading
import time

import pytest

import xtriever

pytestmark = pytest.mark.models


def test_a_python_thread_runs_during_a_search(handle):
    """The worker starts only once the main thread is about to enter the foreign call and
    records when it finished; if the call held the interpreter lock, the worker could not run
    until the search returned, so its finish time would come after the search's (review
    round 1 #3)."""
    handle.search("warm up", xtriever.SearchOptions(k=10, rerank_depth=20))
    go = threading.Event()
    timings = {}

    def work():
        go.wait()
        t0 = time.perf_counter()
        for _ in range(20):
            sum(range(100_000))
        timings["work_s"] = time.perf_counter() - t0
        timings["finished_at"] = time.perf_counter()

    t = threading.Thread(target=work)
    t.start()
    go.set()  # the worker wakes on the next interpreter switch; the search enters the FFI now
    t_search0 = time.perf_counter()
    r = handle.search("what is the capital", xtriever.SearchOptions(k=10, rerank_depth=20))
    search_end = time.perf_counter()
    t.join()
    assert r.hits
    search_s = search_end - t_search0
    assert timings["finished_at"] < search_end, (timings, search_s)
    assert timings["work_s"] < 0.25 * search_s, (timings["work_s"], search_s)


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
