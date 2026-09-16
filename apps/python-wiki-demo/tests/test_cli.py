"""The command line's contract that needs no engine (contracts/cli.md): the empty result
(message, exit 0 — stubbed, since a non-empty index with a dense stage never returns
nothing), argument validation, and the subcommands offered."""

from types import SimpleNamespace

import pytest

from conftest import run_cli
from wikidemo import cli


def _stub_opened():
    return SimpleNamespace(
        paths=SimpleNamespace(artefact="/x"),
        info=SimpleNamespace(documents=1, format_version=2, embedder_load_ms=1, reranker_load_ms=1),
        open_ms=1,
    )


def _empty_run():
    stages = SimpleNamespace(lexical_candidates=0, dense_candidates=0, degraded=None, rerank=None, time_limit_ignored=False)
    response = SimpleNamespace(hits=[], stages=stages, elapsed_ms=3)
    return SimpleNamespace(label="fused", options=None, response=response, wall_ms=4, peak_bytes=5 * 1024 * 1024)


def test_empty_result_prints_the_line_and_exits_0(monkeypatch):
    monkeypatch.setattr(cli, "require", lambda paths, needs: None)
    monkeypatch.setattr(cli, "open_artefact", lambda paths: _stub_opened())

    def fake_run_search(opened, query, **kw):
        yield _empty_run()
        raise AssertionError("the re-ranked call must not run on an empty fused result")

    monkeypatch.setattr(cli, "run_search", fake_run_search)
    code, out, err = run_cli(["search", "the of and"])
    assert code == 0, err
    assert 'no passages found for "the of and"' in out
    assert "stages: lexical 0 · dense 0 · re-rank none" in out
    assert "wall: fused 4 ms · peak resident 5 MB" in out


def test_negative_budget_is_a_usage_error():
    code, out, err = run_cli(["search", "--budget-ms", "-1", "x"])
    assert code == 2 and "must not be negative" in err
    code, out, err = run_cli(["search", "-k", "0", "x"])
    assert code == 2 and "must be at least 1" in err
    code, out, err = run_cli(["search", "--depth", "7", "x"])
    assert code == 2


def test_build_is_not_offered_before_pr_b():
    code, out, err = run_cli(["build", "--out", "/tmp/x"])
    assert code == 2 and "invalid choice: 'build'" in err
    assert set(cli.COMMANDS) == {"search", "about", "measure"}
