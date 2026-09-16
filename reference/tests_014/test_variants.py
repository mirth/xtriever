"""US2: the interpolation variants — rank fusion and linear interpolation within the head."""

import pytest

import gen_003_fixtures as ref003  # noqa: E402
import rerank_study as rs  # noqa: E402

from helpers_014 import K


def head4():
    """Fused ranks 1–4 = a, b, c, d; cross-encoder order c, a, d, b."""
    ce = {"a": 3.0, "b": 1.0, "c": 4.0, "d": 2.0}
    return [
        {"id": i, "pos": p, "r_f": p + 1, "s_f": 1.0 / (61 + p), "s_c": ce[i]}
        for p, i in enumerate("abcd")
    ]


def test_rrf_hand_computed():
    # a: 1/61 + 1/62 = .032522; c: 1/63 + 1/61 = .032266; b: 1/62 + 1/64 = .031754; d: 1/64 + 1/63 = .031498
    assert rs.order_rrf(head4(), ["e", "f"], K) == ["a", "c", "b", "d", "e", "f"]


def test_rrf_ties_by_position():
    head = head4()
    for h in head:
        h["s_c"] = 1.0  # all cross-encoder ranks tie → r_c by position = r_f; scores decrease with r_f
    assert rs.order_rrf(head, [], K) == ["a", "b", "c", "d"]


def test_minmax():
    assert rs.minmax([0.03, 0.02, 0.01]) == pytest.approx([1.0, 0.5, 0.0])
    assert rs.minmax([2.0, 2.0, 2.0]) == [0.0, 0.0, 0.0]
    assert rs.minmax([7.0]) == [0.0]
    assert rs.minmax([]) == []


def lin_head():
    ce = [-1.0, 3.0, 1.0]
    sf = [0.03, 0.02, 0.01]
    return [
        {"id": i, "pos": p, "r_f": p + 1, "s_f": sf[p], "s_c": ce[p]} for p, i in enumerate("xyz")
    ]


def test_lin_hand_computed_at_half():
    # f = [1, .5, 0]; c = [0, 1, .5]; s = [.5, .75, .25] → y, x, z
    assert rs.order_lin(lin_head(), ["w"], K, 0.5) == ["y", "x", "z", "w"]


def test_lin_alpha_zero_is_the_fused_order_and_one_the_cross_encoder_order():
    assert rs.order_lin(lin_head(), [], K, 0.0) == ["x", "y", "z"]
    assert rs.order_lin(lin_head(), [], K, 1.0) == ["y", "z", "x"]


def test_lin_single_candidate_and_constant_column_keep_the_fused_order():
    one = lin_head()[:1]
    assert rs.order_lin(one, ["w"], K, 0.5) == ["x", "w"]
    const = lin_head()
    for h in const:
        h["s_c"] = 2.0
    assert rs.order_lin(const, [], K, 0.75) == ["x", "y", "z"]


@pytest.mark.parametrize("variant", ["replace", "rrf", "lin-0.25", "lin-0.5", "lin-0.75"])
@pytest.mark.parametrize("d", [5, 10, 20, 50])
def test_every_variant_keeps_the_rest_in_fused_order_without_duplicates(explain_lines, positions, variant, d):
    runs = rs.derive(explain_lines, positions, [variant], [d], k=K)
    for line in explain_lines:
        got = runs[(variant, d)][line["query_id"]]
        assert len(got) <= K
        assert len(set(got)) == len(got)
        assert sorted(got) == sorted(line["fused"])
        head_ids = {h["id"] for h in rs.head(line, d, positions)}
        tail = [i for i in got if i not in head_ids]
        assert tail == [i for i in line["fused"] if i not in head_ids]


@pytest.mark.parametrize("variant", ["replace", "rrf", "lin-0.25", "lin-0.5", "lin-0.75"])
def test_recall_100_equals_depth_zero(explain_lines, positions, qrels, variant):
    depth0 = rs.derive(explain_lines, positions, ["replace"], [0], k=K)[("replace", 0)]
    want = rs.score_run(qrels, depth0)["mean_recall_100"]
    for d in (5, 10, 20, 50):
        run = rs.derive(explain_lines, positions, [variant], [d], k=K)[(variant, d)]
        assert rs.score_run(qrels, run)["mean_recall_100"] == pytest.approx(want, abs=1e-12)


def test_registry_names():
    assert list(rs.VARIANTS) == ["replace", "rrf", "lin-0.25", "lin-0.5", "lin-0.75"]
