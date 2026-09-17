"""MaxP aggregation (research D6): a document's rank is its best passage's — first
occurrence in the engine's ordered list — deduplicated and truncated to 100."""

from types import SimpleNamespace

from chunking_study import maxp


def _hit(eid, parent=None):
    chunk = None if parent is None else SimpleNamespace(parent=parent, ordinal=0)
    return SimpleNamespace(external_id=eid, chunk=chunk)


def test_first_occurrence_dedupes():
    hits = [_hit("a#1", "a"), _hit("b#0", "b"), _hit("a#0", "a"), _hit("c#2", "c"), _hit("b#3", "b")]
    docs, short = maxp(hits)
    assert docs == ["a", "b", "c"] and short is True


def test_truncates_to_100():
    hits = [_hit(f"d{i}#0", f"d{i}") for i in range(150)]
    docs, short = maxp(hits)
    assert len(docs) == 100 and docs[0] == "d0" and docs[-1] == "d99" and short is False


def test_whole_hits_use_the_external_id():
    docs, _ = maxp([_hit("x"), _hit("y"), _hit("x")])
    assert docs == ["x", "y"]


def test_exactly_100_is_not_short():
    docs, short = maxp([_hit(f"d{i}") for i in range(100)])
    assert len(docs) == 100 and short is False
