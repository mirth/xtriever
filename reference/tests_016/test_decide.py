"""US3: the FR-007 rule on the `lex2+dense+dot-rr` row."""

import sparse_remeasure as sr  # noqa: E402

V3 = {"scifact": 0.720711, "nfcorpus": 0.362246, "fiqa": 0.390964}  # hybrid-rerank-v3, mean 0.491307


def row(**ndcg):
    return {"variant": "lex2+dense+dot", "reranked": True, "ndcg": ndcg, "mean": sum(ndcg.values()) / 3}


def test_qualifies_at_the_floor_with_no_drop():
    r = row(scifact=0.720711 + 0.0150, nfcorpus=0.362246, fiqa=0.390964)  # mean = 0.496307 ≥ 0.4963
    d = sr.decide(r, V3)
    assert d["qualifies"] is True and "specify" in d["statement"]
    assert d["rule"]["mean_floor"] == 0.4963, "the floor exactly as FR-007 declares it"


def test_a_hair_below_the_floor_does_not_qualify():
    r = row(scifact=0.720711 + 0.0146, nfcorpus=0.362246, fiqa=0.390964)  # mean = 0.496174 < 0.4963
    d = sr.decide(r, V3)
    assert d["qualifies"] is False and d["statement"].startswith("012's GO withdrawn")


def test_a_drop_beyond_the_bound_disqualifies_despite_the_mean():
    r = row(scifact=0.720711 - 0.0051, nfcorpus=0.362246 + 0.02, fiqa=0.390964 + 0.02)
    assert sr.decide(r, V3)["qualifies"] is False


def test_reopen_conditions_always_listed():
    r = row(scifact=0.70, nfcorpus=0.36, fiqa=0.39)
    d = sr.decide(r, V3)
    assert len(d["reopen"]) >= 3 and d["row"] == "lex2+dense+dot-rr"


def test_a_value_between_the_rounded_and_the_derived_floor_qualifies():
    # 0.4963 ≤ mean < 0.496307: the declared floor admits it (review round 1 #1).
    r = row(scifact=0.720711 + 0.014990, nfcorpus=0.362246, fiqa=0.390964)  # mean ≈ 0.496304
    assert r["mean"] >= 0.4963 - 1e-9 and r["mean"] < 0.496307
    assert sr.decide(r, V3)["qualifies"] is True
