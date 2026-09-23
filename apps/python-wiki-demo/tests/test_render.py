"""The text layout is the contract's (contracts/cli.md; research D16), line for line, on
stub responses — no engine needed."""

from pathlib import Path
from types import SimpleNamespace

import xtriever

from wikidemo.hits import Mark, displayed, marks
from wikidemo.render import (
    WARMUP_LINE,
    dropped_line,
    empty_line,
    error_line,
    list_block,
    open_line,
    stage_line,
    wall_line,
)


def _hit(external_id, text, ordinal=None, explain=None, score=0.03, rerank_score=None):
    chunk = None if ordinal is None else SimpleNamespace(parent=external_id.split("#")[0], ordinal=ordinal, byte_start=0, byte_end=1)
    return SimpleNamespace(external_id=external_id, text=text, chunk=chunk, explain=explain, score=score, rerank_score=rerank_score)


def _explain(**kw):
    base = dict(bm25_score=None, bm25_rank=None, dense_score=None, dense_rank=None, fused=0.0, rerank_score=None, rerank_rank=None, rerank_combined=None)
    base.update(kw)
    return SimpleNamespace(**base)


def _stages(lexical=100, dense=100, degraded=None, rerank=None, time_limit_ignored=False):
    return SimpleNamespace(lexical_candidates=lexical, dense_candidates=dense, degraded=degraded, rerank=rerank, time_limit_ignored=time_limit_ignored)


def test_open_line():
    opened = SimpleNamespace(
        paths=SimpleNamespace(artefact=Path("/x/target/xt-wiki")),
        info=SimpleNamespace(documents=427947, format_version=2, embedder_load_ms=169, reranker_load_ms=174),
        open_ms=340,
    )
    assert open_line(opened) == "opened /x/target/xt-wiki (427,947 passages, format 2) · embedder 169 ms · re-ranker 174 ms · open 340 ms · mmap"
    opened.info.reranker_load_ms = None
    assert "re-ranker none" in open_line(opened)
    assert WARMUP_LINE == "warm-up: the first search of a process pages the vectors in"


def test_fused_list_lines():
    hits = displayed([
        _hit("834076#0", "Sky blue\n\nSky blue is a shade of cyan.", ordinal=0),
        _hit("d001", "plain fixture text"),
    ])
    lines = list_block("fused (lexical + dense)", hits, 251)
    assert lines[0] == "fused (lexical + dense), 2 hits, 251 ms"
    assert lines[1] == " 1. Sky blue  834076#0  passage 1"
    assert lines[2] == "    https://simple.wikipedia.org/wiki/Sky%20blue"
    assert lines[3] == "    Sky blue is a shade of cyan."
    assert lines[4] == " 2. d001  d001"
    assert lines[5] == "    plain fixture text"
    assert len(lines) == 6


def test_reranked_list_has_mark_column_and_dropped_line():
    fused_ids = ["834076#0", "2004#0", "2004#1", "5163#3"]
    m, dropped = marks(fused_ids, ["2004#0", "834076#0", "2004#1"])
    hits = displayed([
        _hit("2004#0", "Sky\n\nThe sky.", ordinal=0, rerank_score=8.65),
        _hit("834076#0", "Sky blue\n\nA shade.", ordinal=0),
        _hit("2004#1", "Sky\n\nMany things.", ordinal=1),
    ], marks=m)
    lines = list_block("re-ranked (interpolate α 0.5, depth 10)", hits, 822)
    assert lines[0] == "re-ranked (interpolate α 0.5, depth 10), 3 hits, 822 ms"
    assert lines[1] == " 1. ↑1   Sky  2004#0  passage 1"
    assert lines[4] == " 2. ↓1   Sky blue  834076#0  passage 1"
    assert lines[7] == " 3. =    Sky  2004#1  passage 2"
    assert dropped_line(dropped) == "dropped from the head: 5163#3 (was 4)"
    assert dropped_line([("a", 1), ("b", 2)]) == "dropped from the head: a (was 1), b (was 2)"
    assert dropped_line([]) is None
    [h] = displayed([_hit("x", "T\n\nb")], marks={"x": Mark("new")})
    assert list_block("l", [h], 1)[1] == " 1. new  T  x"


def test_explain_block_prints_eight_features():
    e = _explain(bm25_score=4.5, bm25_rank=1, dense_score=0.25, dense_rank=3, fused=0.0317, rerank_score=8.657302856445312, rerank_rank=1, rerank_combined=1.0)
    hits = displayed([_hit("2004#0", "Sky\n\nThe sky.", ordinal=0, explain=e)])
    lines = list_block("fused", hits, 1, explain=True)
    assert lines[4:12] == [
        "    bm25.score: 4.5000",
        "    bm25.rank: 1",
        "    dense.score: 0.2500",
        "    dense.rank: 3",
        "    fused.score: 0.0317",
        "    rerank.score: 8.6573",
        "    rerank.rank: 1",
        "    rerank.combined: 1.0000",
    ]
    hits = displayed([_hit("d001", "plain", explain=_explain(dense_score=0.1, dense_rank=2, fused=0.01))])
    lines = list_block("fused", hits, 1, explain=True)
    assert lines[3] == "    bm25.score: not seen by this stage"
    assert lines[10] == "    rerank.combined: not seen by this stage"


def test_stage_line_variants():
    assert stage_line(_stages(rerank=SimpleNamespace(candidates=10, scored=10, skipped=None)), 822) == (
        "stages: lexical 100 · dense 100 · re-rank 10 candidates, 10 scored · time limit ignored: no · engine 822 ms"
    )
    assert stage_line(_stages(rerank=None), 251) == "stages: lexical 100 · dense 100 · re-rank none · time limit ignored: no · engine 251 ms"
    budget = xtriever.DegradeReason.BUDGET_EXCEEDED(elapsed_ms=301, limit_ms=300)
    assert stage_line(_stages(dense=None, degraded=SimpleNamespace(stage="dense", reason=budget), rerank=None), 305) == (
        "stages: lexical 100 · dense skipped · re-rank none · degraded: dense (budget exceeded: 301 ms of 300) · time limit ignored: no · engine 305 ms"
    )
    err = xtriever.DegradeReason.STAGE_ERROR(message="boom")
    assert stage_line(_stages(rerank=SimpleNamespace(candidates=10, scored=4, skipped=err), time_limit_ignored=True), 9) == (
        "stages: lexical 100 · dense 100 · re-rank 10 candidates, 4 scored, skipped (stage error: boom) · time limit ignored: yes · engine 9 ms"
    )


def test_wall_line_and_footprint():
    # Decimal megabytes, the ceiling's unit (Feature 026): 612 MiB is 641.7 MB.
    assert wall_line(251, 822, 612 * 1024 * 1024) == "wall: fused 251 ms · re-ranked 822 ms · total 1,073 ms · peak resident 641.7 MB"
    assert wall_line(251, None, 100_000_000) == "wall: fused 251 ms · peak resident 100.0 MB"


def test_multiline_passages_are_indented_on_every_line():
    hits = displayed([_hit("2004#0", "Sky\n\nfirst paragraph\nsecond paragraph", ordinal=0)])
    lines = list_block("fused", hits, 1)
    assert lines[3:5] == ["    first paragraph", "    second paragraph"]


def test_empty_result_line():
    assert empty_line("the of and") == 'no passages found for "the of and"'


def test_error_line():
    assert error_line(xtriever.XtrieverError.Corrupt("not a hybrid index")) == "wikidemo: Corrupt: not a hybrid index"


def test_snippet_cuts_and_says_so():
    hits = displayed([_hit("2004#0", "Sky\n\n" + "x" * 50, ordinal=0)])
    lines = list_block("fused", hits, 1, snippet=10)
    assert lines[0] == "fused, 1 hits, 1 ms, passages cut to 10 characters"
    assert lines[3] == "    " + "x" * 10 + "…"
    lines = list_block("fused", displayed([_hit("2004#0", "Sky\n\nshort", ordinal=0)]), 1, snippet=10)
    assert lines[3] == "    short"
