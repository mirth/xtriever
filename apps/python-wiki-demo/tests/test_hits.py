"""The hit helpers are the Swift package's and the CLI's, case for case (research D3, D4;
spec FR-006, FR-007): the title/passage split, the article URL (the CLI's eleven cases),
the change marks (the iOS rule) and the eight feature names."""

from types import SimpleNamespace

from wikidemo.hits import (
    FEATURE_NAMES,
    Mark,
    displayed,
    features,
    marks,
    render_feature,
    title_and_passage,
    wikipedia_url,
)

BASE = "https://simple.wikipedia.org/wiki/"

# crates/xtriever-cli/src/wiki/url.rs — `derives_the_snapshot_urls`, verbatim.
URL_CASES = [
    ("April", "April"),
    ("Alan Turing", "Alan%20Turing"),
    ("Church (building)", "Church%20%28building%29"),
    ("Dutton's Speedwords", "Dutton%27s%20Speedwords"),
    ("AC/DC", "AC/DC"),
    ("Biel/Bienne", "Biel/Bienne"),
    ("Alliance 90/The Greens", "Alliance%2090/The%20Greens"),
    ("Café", "Caf%C3%A9"),
    ("a~b_c-d.e", "a~b_c-d.e"),
    ("東京", "%E6%9D%B1%E4%BA%AC"),
    ("100% sure?", "100%25%20sure%3F"),
]


def test_title_and_passage_splits_at_first_blank_line():
    assert title_and_passage("T\n\nbody\n\nmore") == ("T", "body\n\nmore")
    assert title_and_passage("Sky\n\n") == ("Sky", "")
    assert title_and_passage("no blank line here") is None
    assert title_and_passage("") is None


def test_wikipedia_url_matches_the_cli_cases():
    for title, want in URL_CASES:
        assert wikipedia_url(title) == BASE + want, title


def test_marks_follow_the_ios_rule():
    got, dropped = marks(["a", "b", "c", "d"], ["b", "a", "c", "e"])
    assert got == {
        "b": Mark("up", 1),
        "a": Mark("down", 1),
        "e": Mark("new"),
        "c": Mark("same"),
    }
    assert dropped == [("d", 4)]
    same, none_dropped = marks(["a", "b"], ["a", "b"])
    assert same == {"a": Mark("same"), "b": Mark("same")}
    assert none_dropped == []
    assert Mark("up", 3).render() == "↑3"
    assert Mark("down", 2).render() == "↓2"
    assert Mark("same").render() == "="
    assert Mark("new").render() == "new"


def test_features_are_the_eight_engine_names_in_order():
    assert FEATURE_NAMES == [
        "bm25.score",
        "bm25.rank",
        "dense.score",
        "dense.rank",
        "fused.score",
        "rerank.score",
        "rerank.rank",
        "rerank.combined",
    ]
    explain = SimpleNamespace(
        bm25_score=None,
        bm25_rank=None,
        dense_score=0.25,
        dense_rank=3,
        fused=0.0317,
        rerank_score=None,
        rerank_rank=None,
        rerank_combined=None,
    )
    got = features(explain)
    assert [n for n, _ in got] == FEATURE_NAMES
    assert dict(got)["dense.rank"] == 3
    assert dict(got)["bm25.score"] is None
    assert render_feature("bm25.score", None) == "not seen by this stage"
    assert render_feature("dense.rank", 3) == "3"
    assert render_feature("fused.score", 0.0317) == "0.0317"
    assert features(None) == [(n, None) for n in FEATURE_NAMES]


def _hit(external_id, text, chunk=None, explain=None, score=0.5, rerank_score=None):
    return SimpleNamespace(
        external_id=external_id,
        text=text,
        chunk=chunk,
        explain=explain,
        score=score,
        rerank_score=rerank_score,
    )


def test_displayed_hit_without_title_line_uses_the_id():
    [d] = displayed([_hit("d001", "plain text, no title line")])
    assert d.rank == 1
    assert d.title == "d001"
    assert d.passage == "plain text, no title line"
    assert d.url is None
    assert d.ordinal is None
    assert d.parent is None
    assert d.mark is None


def test_displayed_hit_with_title_line_and_provenance():
    chunk = SimpleNamespace(parent="2004", ordinal=1, byte_start=10, byte_end=20)
    m, _ = marks(["2004#1", "x"], ["2004#1"])
    [d] = displayed([_hit("2004#1", "Sky\n\nMany things…", chunk=chunk)], marks=m)
    assert (d.title, d.passage) == ("Sky", "Many things…")
    assert d.url == BASE + "Sky"
    assert d.parent == "2004"
    assert d.ordinal == 1
    assert d.mark == Mark("same")
