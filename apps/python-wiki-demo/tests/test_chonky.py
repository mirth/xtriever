"""The ``--chunker chonky`` chunker is the chonky splitter plus provenance (Feature 021,
optional since Feature 023; spec FR-003): the splitter's slices become character ranges
that partition the text, every non-empty chunk becomes one passage with its byte range, and
passages over the embedder's window are counted, not cut. The offset arithmetic is tested
on a stub splitter; the real splitter (``chonky``) must return a partition of real-shaped
articles."""

import pytest

from conftest import CHONKY, EMBEDDER
from wikidemo import record
from wikidemo.chonky_chunker import ChonkyChunker, Splitter
from wikidemo.chunking import WINDOW, BuildError, Window, passage_text

TEXT = "Café au lait.\n\n" + "  " + "Deuxième paragraphe — fin."


class _StubSplitter:
    def __init__(self, pieces):
        self.pieces = pieces

    def __call__(self, text):
        yield from self.pieces


def _splitter(pieces):
    s = Splitter.__new__(Splitter)
    s._splitter = _StubSplitter(pieces)
    return s


def _chunker(splitter, window):
    """A `ChonkyChunker` around a stub splitter — no model, no extra."""
    c = ChonkyChunker.__new__(ChonkyChunker)
    c._splitter, c._window = splitter, window
    return c


def documents_for(article, splitter, window):
    return _chunker(splitter, window).documents_for(article)


class _StubWindow:
    """token_count = words + 2 (the [CLS]/[SEP] positions), or a fixed override."""

    def __init__(self, fixed=None):
        self.fixed = fixed

    def token_count(self, s):
        return self.fixed if self.fixed is not None else len(s.split()) + 2

    def over(self, s):
        return self.token_count(s) > WINDOW


def test_chunks_are_character_ranges_from_the_splitter():
    s = _splitter(["Café au lait.\n\n", "  ", "Deuxième paragraphe — fin."])
    assert s.chunks(TEXT) == [(0, 15), (15, 17), (17, 43)]
    assert WINDOW == 256


def test_partition_is_enforced():
    s = _splitter(["Café au lait.\n\n", "  "])  # does not reach the end
    with pytest.raises(BuildError, match="partition"):
        s.chunks(TEXT)
    s = _splitter(["Café au lait.\n\n", "XX", "Deuxième paragraphe — fin."])  # wrong content
    with pytest.raises(BuildError, match="partition"):
        s.chunks(TEXT)


def test_documents_for_shapes_and_offsets():
    import xtriever

    s = _splitter(["Café au lait.\n\n", "  ", "Deuxième paragraphe — fin."])
    docs, tokens = documents_for({"id": "9", "title": "T", "text": TEXT}, s, _StubWindow())
    assert [d.external_id for d in docs] == ["9#0", "9#1"]  # the whitespace-only chunk emits nothing
    assert docs[0].fields["text"] == xtriever.FieldValue.TEXT("T\n\nCafé au lait.")
    assert docs[1].fields["text"] == xtriever.FieldValue.TEXT("T\n\nDeuxième paragraphe — fin.")
    assert docs[0].fields["title"] == xtriever.FieldValue.TEXT("T")
    raw = TEXT.encode("utf-8")
    c0, c1 = docs[0].chunk, docs[1].chunk
    assert (c0.parent, c0.ordinal, c1.parent, c1.ordinal) == ("9", 0, "9", 1)
    assert raw[c0.byte_start : c0.byte_end].decode("utf-8") == "Café au lait.\n\n"
    assert raw[c1.byte_start : c1.byte_end].decode("utf-8") == "Deuxième paragraphe — fin."
    assert c1.byte_end == len(raw)
    assert len(tokens) == 2 and all(n > 0 for n in tokens)
    assert sum(n > WINDOW for n in tokens) == 0


def test_over_window_is_counted_not_cut():
    s = _splitter(["Café au lait.\n\n", "  ", "Deuxième paragraphe — fin."])
    docs, tokens = documents_for({"id": "9", "title": "T", "text": TEXT}, s, _StubWindow(fixed=300))
    assert len(docs) == 2 and tokens == [300, 300]
    assert sum(n > WINDOW for n in tokens) == 2


def test_passage_text():
    assert passage_text("T", "b") == "T\n\nb"


def test_block_and_label():
    chunker = _chunker(_splitter([""]), _StubWindow())  # the block and the label do not need the model
    assert chunker.block == record.CHONKY_CHUNKER
    assert chunker.label == "chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae…)"


def test_empty_text_yields_no_documents():
    docs, tokens = documents_for({"id": "1", "title": "T", "text": ""}, _splitter([""]), _StubWindow())
    assert docs == [] and tokens == []


def _long_article():
    """The snapshot's first article (~16k characters) — a real long text for the stride
    windows; the model finds no breaks in monotonous synthetic prose, so nothing synthetic
    stands in for it."""
    from conftest import REPO

    snapshot = REPO / "reference/datasets/wiki/simple.jsonl"
    if not snapshot.exists():
        pytest.skip(f"snapshot not on disk: {snapshot}")
    import json

    with snapshot.open(encoding="utf-8") as fh:
        return json.loads(fh.readline())


@pytest.mark.models
@pytest.mark.chonky
def test_real_splitter_partitions_articles():
    chunker = ChonkyChunker(CHONKY, Window(EMBEDDER))
    assert chunker.block == record.CHONKY_CHUNKER and chunker.label.startswith("chonky (")
    splitter = chunker._splitter
    article = _long_article()
    LONG = article["text"]
    assert len(LONG) > 5_000
    texts = [
        "The sky is the appearance of the atmosphere around the surface of the planet.\n\nThe sky is blue because of the random scattering of sunlight by the molecules.",
        "A café is a place that sells coffee.\n\nPeople sit and talk in cafés for hours.",
        LONG,
    ]
    for text in texts:
        ranges = splitter.chunks(text)
        assert ranges[0][0] == 0 and ranges[-1][1] == len(text)
        assert all(a[1] == b[0] for a, b in zip(ranges, ranges[1:]))
        assert "".join(text[s:e] for s, e in ranges) == text
    assert len(splitter.chunks(LONG)) >= 2
    docs, tokens = chunker.documents_for({"id": "7", "title": article["title"], "text": LONG})
    assert len(docs) == len(tokens) >= 2
    import xtriever

    raw = LONG.encode("utf-8")
    for d in docs:
        chunk = raw[d.chunk.byte_start : d.chunk.byte_end].decode("utf-8")
        assert d.fields["text"] == xtriever.FieldValue.TEXT(passage_text(article["title"], chunk.strip()))
