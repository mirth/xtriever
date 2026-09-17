"""The four splitters' pure parts (research D5; spec FR-004): the contract budget and the
title-fills-window case, chonky's partition check, and chonky-bounded's merge-then-bound."""

import pytest

from helpers_022 import MIN_POSITIONS, WINDOW, StubSplitter, words_cost, words_positions
import chunking_study as cs


def test_constants_are_literals():
    assert cs.WINDOW == WINDOW == 256
    assert cs.MIN_POSITIONS == MIN_POSITIONS == 16


def test_split_contract_paragraphs_and_budget():
    text = "one two three four.\n\nfive six seven eight."
    # title takes 250 positions → budget 6 words: each paragraph (4 words) fits alone
    chunks, flag = cs.split_contract(text, words_cost, title_positions=250)
    assert chunks == ["one two three four.", "five six seven eight."]
    assert flag is False
    # a generous budget packs both paragraphs (the 008 joiner is "\n")
    chunks, _ = cs.split_contract(text, words_cost, title_positions=2)
    assert chunks == ["one two three four.\nfive six seven eight."]


def test_split_contract_title_fills_window():
    chunks, flag = cs.split_contract("some text", words_cost, title_positions=WINDOW)
    assert chunks == ["some text"] and flag is True


def test_chonky_passages_partition_and_strip():
    text = "A first paragraph.\n\n  \n\nSecond one."
    splitter = StubSplitter({text: ["A first paragraph.\n\n", "  \n\n", "Second one."]})
    assert cs.chonky_passages(text, splitter) == ["A first paragraph.", "Second one."]
    bad = StubSplitter({text: ["A first paragraph.\n\n"]})
    with pytest.raises(cs.StudyError, match="partition"):
        cs.chonky_passages(text, bad)


def _cost_positions(text):
    return words_positions(text)


def test_bound_and_merge_merges_fragments_into_predecessor():
    first = " ".join(f"a{i}" for i in range(20))  # 22 positions under the stub: a full chunk
    third = " ".join(f"b{i}" for i in range(20))
    chunks = [first, "tiny", third]
    # "tiny" is 1 word + 2 = 3 positions < 16 → joins its predecessor with a newline
    out, kept = cs.bound_and_merge(chunks, words_cost, _cost_positions, title_positions=2)
    assert out == [first + "\ntiny", third]
    assert kept == 0


def test_bound_and_merge_first_fragment_joins_successor():
    out, kept = cs.bound_and_merge(["tiny", "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen"], words_cost, _cost_positions, title_positions=2)
    assert out == ["tiny\none two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen"]
    assert kept == 0


def test_bound_and_merge_rechunks_over_window():
    long = "w" + " w" * 299  # 300 words → over the window with a small title
    out, kept = cs.bound_and_merge([long], words_cost, _cost_positions, title_positions=2)
    budget = WINDOW - 2
    assert len(out) >= 2
    assert all(words_cost(p) <= budget for p in out)
    assert " ".join(out).split() == long.split()  # nothing lost
    assert kept == 0


def test_bound_and_merge_keeps_a_remainder_that_cannot_merge():
    # 254 words packs 254 into one passage (budget 254) and leaves nothing tiny; 260 words →
    # 254 + 6: the 6-word remainder (8 positions < 16) cannot merge back without breaching.
    long = "w" + " w" * 259
    out, kept = cs.bound_and_merge([long], words_cost, _cost_positions, title_positions=2)
    assert [words_cost(p) for p in out] == [254, 6]
    assert kept == 1


def test_passages_for_whole_and_counts():
    doc = {"_id": "d1", "title": "T", "text": "one two three four.\n\nfive six seven eight."}
    passages, counters = cs.passages_for("whole", doc, words_cost, _cost_positions, splitter=None)
    assert passages == [doc["text"]]
    assert counters == {"over_window": 0, "under_16": 0, "title_fills_window": 0}
