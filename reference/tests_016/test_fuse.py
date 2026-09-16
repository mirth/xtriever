"""US1: the engine's RRF — terms, one-list ids, ties by corpus position, three lists."""

import pytest

import sparse_remeasure as sr  # noqa: E402


def test_hand_computed_two_lists(lex, dense, positions):
    fused = sr.fuse([lex, dense], positions)
    scores = dict(fused)
    # d3: lex rank 1, dense rank 2 → 1/61 + 1/62; d1: 1/62 + 1/61 (a tie with d3 → position 1 < 3 → d1 first)
    assert scores["d3"] == pytest.approx(1 / 61 + 1 / 62)
    assert scores["d1"] == pytest.approx(1 / 62 + 1 / 61)
    assert [i for i, _ in fused][:2] == ["d1", "d3"], "tie broken by ascending position"
    # d0: lex 4, dense 4 → 2/64; d7: lex only rank 3 → 1/63; d9: dense only rank 3 → 1/63 → d7 (pos 7) before d9 (pos 9)
    assert scores["d0"] == pytest.approx(2 / 64)
    assert scores["d7"] == pytest.approx(1 / 63) and scores["d9"] == pytest.approx(1 / 63)
    order = [i for i, _ in fused]
    assert order.index("d7") < order.index("d9")
    assert set(order) == set(lex) | set(dense)
    assert len(fused) <= 100


def test_tie_breaks_by_position_not_by_id_string(positions):
    # d10 and d9 at the same rank in two single-id lists: equal sums; "d10" < "d9" as strings,
    # but position 9 < 10, so d9 first.
    fused = sr.fuse([["d10"], ["d9"]], positions)
    assert [i for i, _ in fused] == ["d9", "d10"]


def test_three_lists_add_a_third_term(lex, dense, dot, positions):
    two = dict(sr.fuse([lex, dense], positions))
    three = dict(sr.fuse([lex, dense, dot], positions))
    assert three["d3"] == pytest.approx(two["d3"] + 1 / 62)  # dot rank 2
    assert three["d10"] == pytest.approx(1 / 63)  # dot only, rank 3
    assert three["d5"] == two["d5"]  # not in dot


def test_depth_cuts_the_output(positions):
    lists = [[f"d{i}" for i in range(11)]]
    assert len(sr.fuse(lists, positions, depth=4)) == 4


def test_missing_list_is_skipped(lex, positions):
    # A query absent from one run contributes an empty list: the fusion uses what is present.
    assert [i for i, _ in sr.fuse([lex, []], positions)] == lex
