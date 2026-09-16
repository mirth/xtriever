"""US1: the replace-order derivation equals the engine's rule (006 reference `order_reranked`)."""

import pytest

import gen_006_fixtures as ref006  # noqa: E402  (sys.path from conftest)
import rerank_study as rs  # noqa: E402

from helpers_014 import K


def head_and_rest(fused, scores, d):
    """A head in the study's shape from a 006 case: ids are ints, position = id."""
    head = [
        {"id": i, "pos": i, "r_f": r + 1, "s_f": 1.0 / (60 + r + 1), "s_c": scores[r]}
        for r, i in enumerate(fused[: min(d, len(scores))])
        if scores[r] is not None
    ]
    head_ids = {h["id"] for h in head}
    return head, [i for i in fused if i not in head_ids]


@pytest.mark.parametrize("case", ref006.ORDER_CASES, ids=[c["name"] for c in ref006.ORDER_CASES])
def test_replace_matches_the_006_reference(case):
    head, rest = head_and_rest(case["fused"], case["scores"], case["d"])
    want = [i for i, _ in ref006.order_reranked(case["fused"], case["scores"], case["d"], case["k"])]
    assert rs.order_replace(head, rest, case["k"]) == want


@pytest.mark.parametrize("d", [5, 10, 20, 50])
def test_replace_on_the_fixture_at_every_depth(explain_lines, positions, d):
    line = explain_lines[0]
    head = rs.head(line, d, positions)
    rest = rs.rest(line, head)
    got = rs.order_replace(head, rest, K)
    # Six scored: a depth beyond the scored head uses only the scored entries.
    assert len(head) == min(d, 6)
    if d >= 6:
        # (-score, position): d0 4.0, d1 4.0, d2 3.0, d3 1.0, d5 0.5, d7 -2.0; then d8, d4.
        assert got == ["d0", "d1", "d2", "d3", "d5", "d7", "d8", "d4"]
    else:
        assert got == ["d0", "d1", "d3", "d5", "d7", "d2", "d8", "d4"]


def test_depth_zero_is_the_fused_list(explain_lines, positions):
    line = explain_lines[2]
    head = rs.head(line, 0, positions)
    assert head == []
    assert rs.order_replace(head, rs.rest(line, head), K) == line["fused"]


def test_ties_break_by_ascending_position(explain_lines, positions):
    line = explain_lines[1]  # constant column: the whole head is one tie block
    head = rs.head(line, 6, positions)
    got = rs.order_replace(head, rs.rest(line, head), K)
    assert got[:6] == ["d4", "d5", "d6", "d7", "d8", "d9"]
    assert got[6:] == ["d3", "d2"]


def test_derive_replace_reproduces_hits(explain_lines, positions):
    runs = rs.derive(explain_lines, positions, ["replace"], [6], k=K)
    for line in explain_lines:
        assert runs[("replace", 6)][line["query_id"]] == line["hits"]
