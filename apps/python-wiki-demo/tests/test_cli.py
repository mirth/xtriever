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


def test_the_four_subcommands_are_offered():
    assert set(cli.COMMANDS) == {"search", "about", "build", "measure"}
    code, out, err = run_cli(["build"])
    assert code == 2 and "--out" in err  # required


def test_build_refuses_a_missing_splitter_model_before_loading_torch(tmp_path):
    import sys
    import time

    from conftest import EMBEDDER, RERANKER, REPO

    had_torch = "torch" in sys.modules
    t = time.perf_counter()
    code, out, err = run_cli(
        ["build", "--out", str(tmp_path / "out"), "--limit", "1", "--chonky", "/nonexistent",
         "--snapshot", str(REPO / "reference/datasets/wiki/simple.jsonl"), "--manifest", str(REPO / "reference/datasets/wiki-manifest.json")],
        {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)},
    )
    assert time.perf_counter() - t < 1.0
    if not (EMBEDDER / "model.safetensors").exists() or not (REPO / "reference/datasets/wiki/simple.jsonl").exists():
        assert code == 1 and err.startswith("wikidemo: missing")  # an earlier input is reported first
        return
    assert code == 1, err
    assert err.startswith("wikidemo: missing the chonky splitter model: /nonexistent/model.safetensors\n  produce it with: scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json")
    assert had_torch or "torch" not in sys.modules
    assert not (tmp_path / "out").exists()
