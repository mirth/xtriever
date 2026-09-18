"""The default chunker is the Feature 008 contract, byte for byte (Feature 019 research D5;
Feature 023 spec FR-002): set A (48 cases, cost = words) and set B (nine real articles, cost
from the fixture's `unit_costs` table — no tokenizer needed) from `reference/fixtures/008/`,
and the document shaping of `crates/xtriever-cli/src/wiki/chunking.rs`. The tests are the
019 ones; the pricer stub became a window stub passed to `ContractChunker`."""

import json

import pytest

from conftest import CHUNK_A, CHUNK_B
from wikidemo import record
from wikidemo.chunking import WINDOW, BuildError, passage_text
from wikidemo.contract import ContractChunker, chunk, words


def word_cost(s: str) -> int:
    return len(words(s, 0, len(s)))


@pytest.mark.skipif(not CHUNK_A.exists(), reason=f"{CHUNK_A} missing")
def test_set_a_byte_identical():
    cases = json.loads(CHUNK_A.read_text(encoding="utf-8"))
    assert len(cases) == 48
    for case in cases:
        got = chunk(case["body"], case["budget"], word_cost)
        assert got == case["passages"], (case["name"], case["budget"])


@pytest.mark.skipif(not CHUNK_B.exists(), reason=f"{CHUNK_B} missing")
def test_set_b_byte_identical():
    data = json.loads(CHUNK_B.read_text(encoding="utf-8"))
    unit_costs = data["unit_costs"]
    assert len(data["articles"]) == 9
    priced = 0
    for article in data["articles"]:

        def cost(unit):
            nonlocal priced
            priced += 1
            return unit_costs[unit]

        got = chunk(article["body"], article["budget"], cost)
        assert got == article["passages"], article["title"]
    assert priced > 0


class _Window:
    """A stub window: `token_count` = words + 2 (the [CLS]/[SEP] positions), so the chunker's
    `cost` = words; an optional override for one title."""

    def __init__(self, title=None, title_positions=None):
        self.title, self.title_positions = title, title_positions

    def token_count(self, s):
        if self.title is not None and s == self.title:
            return self.title_positions
        return word_cost(s) + 2


def _documents_for(article, window):
    return ContractChunker(window).documents_for(article)


def test_block_and_label():
    chunker = ContractChunker(_Window())
    assert chunker.block == record.CONTRACT_CHUNKER
    assert chunker.label == "008 contract (256 - token_count(title))"


def test_title_that_fills_the_window_is_an_error():
    article = {"id": "7", "url": "https://simple.wikipedia.org/wiki/Title", "title": "Title", "text": "Some body."}
    with pytest.raises(BuildError) as e:
        _documents_for(article, _Window("Title", WINDOW))
    assert "article 7" in str(e.value) and "256" in str(e.value)
    assert WINDOW == 256


def test_documents_for_shapes():
    import xtriever

    article = {"id": "42", "url": "x", "title": "Two paragraphs", "text": "One two three four.\n\nFive six seven eight nine."}
    # budget = 256 - token_count(title) = 256 - 4 = 252 words: both paragraphs fit in one passage.
    docs, positions = _documents_for(article, _Window())
    assert len(docs) == 1
    d = docs[0]
    assert isinstance(d, xtriever.Document)
    assert d.external_id == "42#0"
    assert d.fields["title"] == xtriever.FieldValue.TEXT("Two paragraphs")
    assert d.fields["text"] == xtriever.FieldValue.TEXT(passage_text("Two paragraphs", "One two three four.\nFive six seven eight nine."))
    assert d.chunk == xtriever.ChunkInfo(parent="42", ordinal=0, byte_start=0, byte_end=len(article["text"].encode("utf-8")))
    assert positions == [word_cost("Two paragraphs\n\nOne two three four.\nFive six seven eight nine.") + 2]


def test_documents_for_splits_at_the_budget():
    # A window whose title costs 252 positions leaves a budget of 4 words: one passage per paragraph.
    article = {"id": "42", "url": "x", "title": "Two paragraphs", "text": "One two three four.\n\nFive six seven eight."}
    docs, positions = _documents_for(article, _Window("Two paragraphs", 252))
    assert [d.external_id for d in docs] == ["42#0", "42#1"]
    assert docs[1].chunk.ordinal == 1 and docs[1].chunk.byte_start == len("One two three four.\n\n")
    assert len(positions) == 2


def test_over_window_is_zero_by_construction():
    # Every passage's positions = its cost + the title's positions + the [CLS]/[SEP] pair,
    # and cost ≤ budget = WINDOW − title positions — so no passage can exceed the window.
    text = "\n\n".join(" ".join(f"w{i}" for i in range(k)) + "." for k in (3, 40, 120, 255, 300))
    article = {"id": "5", "url": "x", "title": "T", "text": text}
    docs, positions = _documents_for(article, _Window())
    assert len(docs) >= 2  # it does split
    assert all(n <= WINDOW for n in positions), positions
