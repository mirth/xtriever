"""Feature 016 checks: the engine's RRF with its tie rule, score sourcing (engine vs reference),
the decision rule, and a synthetic end-to-end. Red until `sparse_remeasure` exists."""

import json
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "reference"))

from helpers_016 import CORPUS_IDS  # noqa: E402  (a plain module: no `conftest` name clash with tests_014)


@pytest.fixture
def positions():
    return {i: p for p, i in enumerate(CORPUS_IDS)}


@pytest.fixture
def lex():
    return ["d3", "d1", "d7", "d0", "d5", "d2"]


@pytest.fixture
def dense():
    return ["d1", "d3", "d9", "d0", "d8", "d4"]


@pytest.fixture
def dot():
    return ["d9", "d3", "d10", "d6", "d1", "d8"]


@pytest.fixture
def explain_line(lex, dense):
    """A synthetic explain line for query `q1`: the engine's fused order of `lex` + `dense`
    with its fused scores, and cross-encoder scores for the first four fused ids."""
    import sparse_remeasure as sr  # noqa: E402 (red until the module exists)

    fused = sr.fuse([lex, dense], {i: p for p, i in enumerate(CORPUS_IDS)})
    scores = {"d1": 3.0, "d3": 1.0, "d0": 2.5, "d7": -1.0}
    rerank = sorted(((i, s) for i, s in scores.items()), key=lambda t: (-t[1], CORPUS_IDS.index(t[0])))
    return {
        "query_id": "q1",
        "lexical": [[r + 1, i] for r, i in enumerate(lex)],
        "dense": [[r + 1, i] for r, i in enumerate(dense)],
        "fused": [i for i, _ in fused],
        "fused_scores": [s for _, s in fused],
        "rerank": [[r + 1, i, s] for r, (i, s) in enumerate(rerank)],
        "hits": [i for i, _ in fused],
    }


@pytest.fixture
def qrels():
    return {"q1": {"d3": 2, "d9": 1, "d6": 1}}


class StubScorer:
    """A deterministic stand-in for the 006 reference: the score is the doc's position / 10."""

    def __init__(self):
        self.calls = 0

    def score(self, query: str, passage: str) -> float:
        self.calls += 1
        return CORPUS_IDS.index(passage) / 10.0  # the "passage" in tests is the doc id


@pytest.fixture
def stub_scorer():
    return StubScorer()
