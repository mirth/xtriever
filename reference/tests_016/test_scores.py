"""US2: head scores come from the engine where covered and from the reference otherwise,
cached; the re-ranking is 014's rule with the tail in fused order."""

import json

import pytest

import sparse_remeasure as sr  # noqa: E402
from helpers_016 import CORPUS_IDS


def test_head_scores_source_and_cache(tmp_path, explain_line, dot, positions, stub_scorer):
    lex = [i for _, i in explain_line["lexical"]]
    dense = [i for _, i in explain_line["dense"]]
    fused = sr.fuse([lex, dense, dot], positions)
    cache = sr.ScoreCache(tmp_path / "reference-scores.jsonl")
    head = sr.head_scores(fused, explain_line, cache, stub_scorer, query_text="q", passages={i: i for i in CORPUS_IDS}, depth=20)
    covered = {i for _, i, _ in explain_line["rerank"]}
    for hid, score, source in head:
        if hid in covered:
            assert source == "engine"
            assert score == dict((i, s) for _, i, s in explain_line["rerank"])[hid]
        else:
            assert source == "reference"
            assert score == CORPUS_IDS.index(hid) / 10.0
    uncovered = [h for h in head if h[2] == "reference"]
    assert uncovered, "the dot list brings candidates the engine never scored"
    assert stub_scorer.calls == len(uncovered)
    # Cached: a second pass scores nothing new.
    cache2 = sr.ScoreCache(tmp_path / "reference-scores.jsonl")
    second = sr.StubCounting(stub_scorer)
    head2 = sr.head_scores(fused, explain_line, cache2, second, query_text="q", passages={i: i for i in CORPUS_IDS}, depth=20)
    assert head2 == head and second.calls == 0
    lines = (tmp_path / "reference-scores.jsonl").read_text().splitlines()
    assert len(lines) == len(uncovered) and json.loads(lines[0])["query_id"] == "q1"


def test_rerank_fused_is_014s_rule_and_keeps_the_tail(explain_line, dot, positions, stub_scorer, tmp_path):
    lex = [i for _, i in explain_line["lexical"]]
    dense = [i for _, i in explain_line["dense"]]
    fused = sr.fuse([lex, dense, dot], positions)
    cache = sr.ScoreCache(tmp_path / "c.jsonl")
    head = sr.head_scores(fused, explain_line, cache, stub_scorer, query_text="q", passages={i: i for i in CORPUS_IDS}, depth=3)
    ordered = sr.rerank_fused(fused, head, positions, depth=3)
    head_ids = {h[0] for h in head}
    assert set(ordered[:3]) == head_ids
    assert ordered[3:] == [i for i, _ in fused if i not in head_ids], "tail in fused order"
    assert sr.reference_share(head) == pytest.approx(sum(h[2] == "reference" for h in head) / len(head))
