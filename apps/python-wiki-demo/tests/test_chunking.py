"""The shared chunking module (Feature 023; spec FR-001, FR-005): the window, the passage
shape, and the chooser — ``make_chunker`` returns the 008 contract chunker by default
without ever touching the chonky extra, the chonky chunker on request, and refuses any
other name."""

import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from conftest import EMBEDDER
from wikidemo import record
from wikidemo.chunking import CHUNKERS, DEFAULT_CHUNKER, WINDOW, BuildError, Window, make_chunker, passage_text


class _StubWindow:
    """token_count = words + 2 (the [CLS]/[SEP] positions)."""

    def token_count(self, s):
        return len(s.split()) + 2


PATHS = SimpleNamespace(chonky=Path("/nonexistent/chonky"))


def test_constants_and_passage_text():
    assert WINDOW == 256
    assert CHUNKERS == ("contract", "chonky")
    assert DEFAULT_CHUNKER == "contract"
    assert passage_text("T", "b") == "T\n\nb"


def test_default_chunker_is_the_contract_and_never_imports_the_extra(monkeypatch):
    monkeypatch.setitem(sys.modules, "chonky", None)  # an `import chonky` would now raise
    chunker = make_chunker("contract", PATHS, _StubWindow())
    assert callable(chunker.documents_for)
    assert chunker.block == record.CONTRACT_CHUNKER
    assert chunker.label == "008 contract (256 - token_count(title))"
    docs, positions = chunker.documents_for({"id": "1", "title": "T", "text": "One two.\n\nThree four."})
    assert [d.external_id for d in docs] == ["1#0"] and positions == [len("T\n\nOne two.\nThree four.".split()) + 2]


def test_unknown_chunker_is_refused():
    with pytest.raises(BuildError) as e:
        make_chunker("whole", PATHS, _StubWindow())
    assert "whole" in str(e.value) and "contract" in str(e.value) and "chonky" in str(e.value)


@pytest.mark.models
def test_window_counts_positions_with_the_embedder_tokenizer():
    window = Window(EMBEDDER)
    assert window.token_count("a b") > 2
    assert window.token_count("") == 2  # [CLS] [SEP]
