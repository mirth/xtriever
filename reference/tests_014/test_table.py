"""US3: the decision rule (research D7); the explain reader's refusal; the run round trip."""

import json

import pytest

import rerank_study as rs  # noqa: E402

DEPTH0 = {"scifact": 0.7144, "nfcorpus": 0.3535, "fiqa": 0.3692}


def row(variant, depth, **ndcg):
    return {"variant": variant, "depth": depth, "ndcg": ndcg, "mean": sum(ndcg.values()) / 3}


def test_decide_floor_and_drop():
    ok = row("rrf", 10, scifact=0.7144, nfcorpus=0.3535 + 0.0075, fiqa=0.3692 + 0.0075)  # mean ≈ 0.4840
    assert ok["mean"] >= 0.4818
    just_below = row("rrf", 5, scifact=0.7144 - 0.0003, nfcorpus=0.3535, fiqa=0.3692)  # mean ≈ 0.4789
    drop = row("replace", 10, scifact=0.7144 - 0.0051, nfcorpus=0.3535 + 0.02, fiqa=0.3692 + 0.02)  # mean high, one drop
    d = rs.decide([ok, just_below, drop], DEPTH0)
    assert [r["variant"] + "-d" + str(r["depth"]) for r in d["qualifying"]] == ["rrf-d10"]
    assert d["winner"]["variant"] == "rrf" and d["winner"]["depth"] == 10
    assert d["rule"] == {"mean_floor": pytest.approx(0.4818), "max_drop": 0.005, "default": "replace-d20", "default_mean": 0.4768}


def test_decide_exact_floor_qualifies_and_a_hair_below_does_not():
    # depth-0 mean is 1.4371 / 3 = 0.47903; the floor 0.4818 needs +0.0083 in total
    at = row("rrf", 20, scifact=0.7144 + 0.0083, nfcorpus=0.3535, fiqa=0.3692)  # mean = 1.4454 / 3 = 0.4818
    below = row("rrf", 50, scifact=0.7144 + 0.0080, nfcorpus=0.3535, fiqa=0.3692)  # mean = 1.4451 / 3 = 0.4817
    d = rs.decide([at, below], DEPTH0)
    assert [r["depth"] for r in d["qualifying"]] == [20]


def test_decide_ties_by_smaller_depth():
    a = row("lin-0.5", 20, scifact=0.72, nfcorpus=0.36, fiqa=0.38)
    b = row("lin-0.5", 10, scifact=0.72, nfcorpus=0.36, fiqa=0.38)
    d = rs.decide([a, b], DEPTH0)
    assert d["winner"]["depth"] == 10


def test_decide_none_qualifies():
    d = rs.decide([row("replace", 20, scifact=0.6954, nfcorpus=0.3609, fiqa=0.3742)], DEPTH0)
    assert d["qualifying"] == [] and d["winner"] is None
    assert d["statement"] == "no configuration qualifies; the default stays"


def test_read_explain_refuses_an_export_without_fused_scores(tmp_path):
    p = tmp_path / "old.jsonl"
    p.write_text(json.dumps({"query_id": "q", "fused": ["a"], "rerank": [[1, "a", 0.1]], "hits": ["a"]}) + "\n")
    with pytest.raises(ValueError, match="explain export predates 014; re-run with the current `beir`"):
        rs.read_explain(p)


def test_read_explain_reads(explain_path):
    lines = rs.read_explain(explain_path)
    assert [l["query_id"] for l in lines] == ["q1", "q2", "q3"]


def test_run_round_trip(tmp_path):
    run = {"q2": ["b", "a"], "q1": ["a", "c", "b"]}
    p = tmp_path / "run.jsonl"
    rs.write_run(p, run)
    assert rs.read_run(p) == run
    assert p.read_text().splitlines()[0] == '{"query_id": "q1", "doc_ids": ["a", "c", "b"]}'
