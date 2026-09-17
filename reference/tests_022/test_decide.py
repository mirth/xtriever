"""The decision rule (spec US4; research D9) at its boundaries, on synthetic score rows."""

from helpers_022 import MAX_DROP, MEAN_GAIN, RECALL_DROP
import chunking_study as cs


def _rows(whole, chunkers):
    """rows: {(variant, dataset): (ndcg_reranked, recall_reranked)}"""
    rows = {}
    for ds, (n, r) in whole.items():
        rows[("whole", ds)] = {"ndcg_10": n, "recall_100": r}
    for v, per in chunkers.items():
        for ds, (n, r) in per.items():
            rows[(v, ds)] = {"ndcg_10": n, "recall_100": r}
    return rows


WHOLE = {"scifact": (0.70, 0.95), "nfcorpus": (0.36, 0.30), "fiqa": (0.39, 0.70)}


def test_constants():
    assert (cs.MEAN_GAIN, cs.MAX_DROP, cs.RECALL_DROP) == (MEAN_GAIN, MAX_DROP, RECALL_DROP) == (0.005, 0.005, 0.005)


def test_recommended_at_exactly_the_gain():
    rows = _rows(WHOLE, {"contract": {"scifact": (0.705, 0.95), "nfcorpus": (0.365, 0.30), "fiqa": (0.395, 0.70)}})
    d = cs.decide(rows)
    v = d["variants"]["contract"]
    assert v["scope"] == "three-way" and v["recommended"] is True
    assert abs(v["delta_mean"] - 0.005) < 1e-9
    assert d["best_chunker"] == "contract"


def test_not_recommended_below_the_gain():
    rows = _rows(WHOLE, {"contract": {"scifact": (0.7049, 0.95), "nfcorpus": (0.3649, 0.30), "fiqa": (0.3949, 0.70)}})
    assert cs.decide(rows)["variants"]["contract"]["recommended"] is False


def test_not_recommended_when_a_dataset_drops():
    rows = _rows(WHOLE, {"contract": {"scifact": (0.72, 0.95), "nfcorpus": (0.3549, 0.30), "fiqa": (0.41, 0.70)}})
    assert cs.decide(rows)["variants"]["contract"]["recommended"] is False


def test_not_recommended_when_recall_drops():
    rows = _rows(WHOLE, {"contract": {"scifact": (0.72, 0.95), "nfcorpus": (0.37, 0.30), "fiqa": (0.41, 0.6847)}})
    v = cs.decide(rows)["variants"]["contract"]
    assert v["recommended"] is False and v["delta_recall"] < -RECALL_DROP


def test_ties_go_to_the_non_neural_chunker():
    same = {"scifact": (0.71, 0.95), "nfcorpus": (0.37, 0.30), "fiqa": (0.40, 0.70)}
    rows = _rows(WHOLE, {"chonky": dict(same), "contract": dict(same), "chonky-bounded": dict(same)})
    assert cs.decide(rows)["best_chunker"] == "contract"


def test_two_way_scope_uses_the_datasets_present():
    rows = _rows(WHOLE, {"chonky": {"scifact": (0.71, 0.95), "nfcorpus": (0.37, 0.30)}})
    v = cs.decide(rows)["variants"]["chonky"]
    assert v["scope"] == "two-way" and v["datasets"] == ["nfcorpus", "scifact"]
    assert abs(v["mean_ndcg_10"] - 0.54) < 1e-9 and abs(v["whole_mean_ndcg_10"] - 0.53) < 1e-9
    assert v["recommended"] is True


def test_decision_keys():
    rows = _rows(WHOLE, {"contract": {"scifact": (0.71, 0.95), "nfcorpus": (0.37, 0.30), "fiqa": (0.40, 0.70)}})
    d = cs.decide(rows)
    assert set(d) == {"variants", "best_chunker", "constants", "rule"}
    assert set(d["variants"]["contract"]) == {"datasets", "scope", "mean_ndcg_10", "whole_mean_ndcg_10", "delta_mean", "deltas", "delta_recall", "recommended"}
    assert d["constants"] == {"mean_gain": 0.005, "max_drop": 0.005, "recall_drop": 0.005, "min_positions": 16, "window": 256}
