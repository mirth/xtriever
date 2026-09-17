"""Run and score files (research D7; contracts/study.md): cell names, the JSONL run shape,
the anchor comparison on synthetic reports."""

import json

from helpers_022 import REPO
import chunking_study as cs


def test_cell_names_and_paths():
    assert cs.cell_name("contract", 20, 300, "scifact") == "contract-d20@300.scifact"
    assert cs.cell_name("whole", 0, 100, "fiqa") == "whole-d0@100.fiqa"
    assert cs.run_path("whole", 0, 100, "fiqa") == cs.RUNS_DIR / "whole-d0@100.fiqa.jsonl"
    assert cs.score_path("whole", 0, 100, "fiqa") == cs.RUNS_DIR / "whole-d0@100.fiqa.json"
    assert cs.RUNS_DIR == REPO / "specs/022-chunking-study/runs"


def test_run_round_trip(tmp_path):
    run = {"q2": ["b", "a"], "q1": ["a"]}
    path = tmp_path / "x.jsonl"
    cs.write_run(path, run)
    lines = path.read_text().splitlines()
    assert json.loads(lines[0]) == {"query_id": "q1", "doc_ids": ["a"]}  # sorted by query id
    assert cs.read_run(path) == run


def _report(values):
    per = {q: {"ndcg_10": n, "recall_100": r} for q, (n, r) in values.items()}
    n = len(per)
    return {
        "per_query": per,
        "scored_queries": n,
        "mean_ndcg_10": sum(v["ndcg_10"] for v in per.values()) / n,
        "mean_recall_100": sum(v["recall_100"] for v in per.values()) / n,
    }


def test_check_anchor():
    want = _report({"q1": (0.5, 1.0), "q2": (0.25, 0.5)})
    assert cs.check_anchor("x", _report({"q1": (0.5, 1.0), "q2": (0.25, 0.5)}), want) is None
    msg = cs.check_anchor("x", _report({"q1": (0.5 + 1e-5, 1.0), "q2": (0.25, 0.5)}), want)
    assert msg is not None and "q1" in msg
    msg = cs.check_anchor("x", _report({"q1": (0.5, 1.0)}), want)
    assert msg is not None and "query sets" in msg


def test_score_file_carries_short_queries(tmp_path, monkeypatch):
    monkeypatch.setattr(cs, "RUNS_DIR", tmp_path)
    qrels = {"q1": {"a": 1}, "q2": {"b": 1}}
    cs.write_run(cs.run_path("whole", 0, 300, "toy"), {"q1": ["a", "x"], "q2": ["y", "b"]})
    cs.write_meta("whole", 0, 300, "toy", {"queries": 2, "short_queries": 2})
    report = cs.score_cell("whole", 0, 300, "toy", qrels)
    assert report["scored_queries"] == 2 and report["short_queries"] == 2 and report["queries"] == 2
    assert set(report) >= {"per_query", "mean_ndcg_10", "mean_recall_100", "cell"}
    assert report["cell"] == "whole-d0@300.toy"
    assert cs.score_path("whole", 0, 300, "toy").exists()
