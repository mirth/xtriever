"""The document shaping is the baselines' (research D3): `title + " " + text`, empty parts omitted."""

from chunking_study import join_title_text, passage_contents


def test_join_title_text():
    assert join_title_text("T", "x y") == "T x y"
    assert join_title_text("", "x") == "x"
    assert join_title_text("T", "") == "T"
    assert join_title_text("", "") == ""


def test_passage_contents_is_the_same_join():
    assert passage_contents("T", "chunk") == "T chunk"
    assert passage_contents("", "chunk") == "chunk"
