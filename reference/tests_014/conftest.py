"""Feature 014 checks: the offline derivation of re-rank variants from a depth-50 explain
export, against the 006 order rule and hand-computed cases. Red until `rerank_study` exists."""

import json
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "reference"))

K = 100

# Ten corpus documents; a document's internal id is its corpus position (eval `build`).
CORPUS_IDS = [f"d{i}" for i in range(10)]


def explain_line(qid, fused, fused_scores, rerank_scores):
    """`fused` ids in fused order; `rerank_scores` for the first len(rerank_scores) of them,
    exported as `[rank, id, score]` in cross-encoder order (rank 1 = best), as `beir` does."""
    scored = list(zip(fused[: len(rerank_scores)], rerank_scores))
    scored.sort(key=lambda t: (-t[1], CORPUS_IDS.index(t[0])))
    return {
        "query_id": qid,
        "lexical": [],
        "dense": [],
        "fused": list(fused),
        "fused_scores": list(fused_scores),
        "rerank": [[r + 1, i, s] for r, (i, s) in enumerate(scored)],
        "hits": [i for i, _ in scored] + [i for i in fused if i not in {x for x, _ in scored}],
    }


@pytest.fixture
def corpus_ids():
    return list(CORPUS_IDS)


@pytest.fixture
def positions():
    return {i: p for p, i in enumerate(CORPUS_IDS)}


@pytest.fixture
def explain_lines():
    """Three queries, eight fused candidates each, the first six scored by the cross-encoder."""
    return [
        explain_line(
            "q1",
            ["d3", "d1", "d7", "d0", "d5", "d2", "d8", "d4"],
            [0.032, 0.031, 0.030, 0.020, 0.019, 0.018, 0.017, 0.016],
            [1.0, 4.0, -2.0, 4.0, 0.5, 3.0],  # d1 and d0 tie at 4.0 → d0 (position 0) first
        ),
        explain_line(
            "q2",
            ["d9", "d8", "d7", "d6", "d5", "d4", "d3", "d2"],
            [0.03, 0.029, 0.028, 0.027, 0.026, 0.025, 0.024, 0.023],
            [2.0, 2.0, 2.0, 2.0, 2.0, 2.0],  # a constant cross-encoder column
        ),
        explain_line(
            "q3",
            ["d0", "d2", "d4", "d6", "d8", "d1", "d3", "d5"],
            [0.05, 0.04, 0.03, 0.02, 0.01, 0.009, 0.008, 0.007],
            [-1.0, 3.0, 1.0, 0.0, 2.5, -3.0],
        ),
    ]


@pytest.fixture
def explain_path(tmp_path, explain_lines):
    p = tmp_path / "explain-d50.jsonl"
    p.write_text("".join(json.dumps(l) + "\n" for l in explain_lines))
    return p


@pytest.fixture
def qrels():
    return {
        "q1": {"d0": 2, "d5": 1, "d8": 1},
        "q2": {"d2": 1, "d9": 1},
        "q3": {"d4": 2, "d1": 1, "d9": 1},  # d9 is never retrieved for q3
    }
