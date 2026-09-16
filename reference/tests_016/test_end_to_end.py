"""Synthetic end-to-end: lists → fuse → head scores → re-rank → run files; Recall@100 equal;
the anchor variant re-ranked with covered scores equals 014's rule on the explain line."""

import pytest

import gen_003_fixtures as ref003  # noqa: E402
import rerank_study as rs  # noqa: E402
import sparse_remeasure as sr  # noqa: E402
from helpers_016 import CORPUS_IDS


def test_pipeline_on_synthetic_lists(tmp_path, explain_line, dot, positions, stub_scorer, qrels):
    lex = [i for _, i in explain_line["lexical"]]
    dense = [i for _, i in explain_line["dense"]]
    fused = sr.fuse([lex, dense, dot], positions)
    plain = {"q1": [i for i, _ in fused]}
    cache = sr.ScoreCache(tmp_path / "c.jsonl")
    head = sr.head_scores(fused, explain_line, cache, stub_scorer, query_text="q", passages={i: i for i in CORPUS_IDS}, depth=4)
    rr = {"q1": sr.rerank_fused(fused, head, positions, depth=4)}
    rs.write_run(tmp_path / "plain.jsonl", plain)
    rs.write_run(tmp_path / "rr.jsonl", rr)
    assert rs.read_run(tmp_path / "rr.jsonl") == rr
    a, b = ref003.reference(qrels, plain), ref003.reference(qrels, rr)
    assert a["mean_recall_100"] == pytest.approx(b["mean_recall_100"])
    assert sorted(rr["q1"]) == sorted(plain["q1"])


def test_anchor_variant_reranked_with_covered_scores_is_014s_rule(tmp_path, explain_line, positions, stub_scorer):
    lex = [i for _, i in explain_line["lexical"]]
    dense = [i for _, i in explain_line["dense"]]
    fused = sr.fuse([lex, dense], positions)
    assert [i for i, _ in fused] == explain_line["fused"], "the fusion reproduces the engine's fused order"
    cache = sr.ScoreCache(tmp_path / "c.jsonl")
    head = sr.head_scores(fused, explain_line, cache, stub_scorer, query_text="q", passages={i: i for i in CORPUS_IDS}, depth=4)
    assert all(src == "engine" for _, _, src in head) and stub_scorer.calls == 0
    got = sr.rerank_fused(fused, head, positions, depth=4)
    h = rs.head(explain_line, 4, positions)
    want = rs.order_lin(h, rs.rest(explain_line, h), 100, 0.5)
    assert got == want
