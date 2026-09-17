"""The minimal demo: one file within budget, the usage exit, the contract's hit lines, a
corpus of the right shape, and — with the models — hits that are the engine's, bit for bit,
and the same on a second run (spec FR-005–FR-007; contracts/example.md)."""

import shutil
import tempfile
from types import SimpleNamespace

import pytest
import xtriever

from conftest import DEMO, EMBEDDER, RERANKER, f32_bits, f64_bits, load_demo


def test_line_budget():
    assert DEMO.exists(), DEMO
    lines = DEMO.read_text(encoding="utf-8").splitlines()
    assert len(lines) <= 80, f"{len(lines)} lines"


def test_usage_exit_2(monkeypatch, capsys):
    demo = load_demo()

    def boom(*a, **k):
        raise AssertionError("no model must load on a usage error")

    monkeypatch.setattr(xtriever.IndexHandle, "create", boom)
    assert demo.main([]) == 2
    assert capsys.readouterr().err.strip() == 'usage: demo.py "your question"'
    assert demo.main(["a", "b"]) == 2


def test_print_hits_on_stubs(capsys):
    demo = load_demo()
    hits = [
        SimpleNamespace(external_id="doc-03", score=0.0328, rerank_score=None, text="Honey bees\n\nBees make honey."),
        SimpleNamespace(external_id="doc-07", score=0.0301, rerank_score=8.6573, text="Tides\n\nThe sea rises."),
    ]
    demo.print_hits("fused (lexical + dense)", hits)
    out = capsys.readouterr().out.splitlines()
    assert out == [
        "fused (lexical + dense), 2 hits",
        " 1. doc-03  score=0.0328  Honey bees",
        " 2. doc-07  score=0.0301  rerank=8.6573  Tides",
    ]


def test_corpus_shape():
    demo = load_demo()
    assert 8 <= len(demo.DOCS) <= 12
    ids = [i for i, _ in demo.DOCS]
    assert len(set(ids)) == len(ids)
    for _, text in demo.DOCS:
        title, sep, body = text.partition("\n\n")
        assert sep and title.strip() and body.strip(), text


def _direct(query, docs):
    """The same documents through the package directly — the oracle."""
    d = tempfile.mkdtemp()
    try:
        config = xtriever.IndexConfig(
            fields=[xtriever.FieldDef(name="contents", kind=xtriever.FieldKind.TEXT(analyzer="standard_en"))],
            dense_fields=["contents"],
        )
        h = xtriever.IndexHandle.create(d, config, str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP)
        h.add([xtriever.Document(external_id=i, fields={"contents": xtriever.FieldValue.TEXT(t)}) for i, t in docs])
        h.commit()
        h.merge()
        fused = h.search(query, xtriever.SearchOptions(k=5, rerank_depth=0))
        reranked = h.search(query, xtriever.SearchOptions(k=5, rerank_depth=10))
        return fused, reranked
    finally:
        shutil.rmtree(d)


def _tuples(response):
    return [(h.external_id, f64_bits(h.score), f32_bits(h.rerank_score)) for h in response.hits]


@pytest.mark.models
def test_hits_are_the_engines():
    demo = load_demo()
    query = "how do bees make honey"
    fused, reranked = demo.run(query)
    assert len(fused.hits) == 5 and len(reranked.hits) == 5
    direct_fused, direct_reranked = _direct(query, demo.DOCS)
    assert _tuples(fused) == _tuples(direct_fused)
    assert _tuples(reranked) == _tuples(direct_reranked)
    assert all(h.rerank_score is not None for h in reranked.hits)
    again_fused, again_reranked = demo.run(query)
    assert _tuples(again_fused) == _tuples(fused) and _tuples(again_reranked) == _tuples(reranked)
