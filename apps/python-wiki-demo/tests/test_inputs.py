"""Inputs resolve flag → environment → default against the repository root, and a missing
one is named with the command that produces it before anything loads (research D11; spec
FR-001, FR-003; contracts/cli.md)."""

import time
from types import SimpleNamespace

from conftest import REPO, run_cli
from wikidemo.inputs import (
    PRODUCERS,
    MissingInput,
    UnusableInput,
    first_missing,
    repo_root,
    require,
    resolve,
    weights,
)


def _args(**kw):
    base = dict(
        artefact=None,
        embedder=None,
        reranker=None,
        snapshot=None,
        manifest=None,
        expected=None,
        queries=None,
        chonky=None,
    )
    base.update(kw)
    return SimpleNamespace(**base)


def test_defaults_resolve_against_the_repo_root(monkeypatch):
    for var in ("XTRIEVER_WIKI_ARTEFACT", "XTRIEVER_MODEL_DIR", "XTRIEVER_RERANK_MODEL_DIR", "XTRIEVER_CHONKY_MODEL_DIR"):
        monkeypatch.delenv(var, raising=False)
    assert repo_root() == REPO
    p = resolve(_args())
    assert p.artefact == REPO / "target/xt-wiki"
    assert p.index_dir == REPO / "target/xt-wiki/index"
    assert p.corpus_json == REPO / "target/xt-wiki/index/corpus.json"
    assert p.attribution == REPO / "target/xt-wiki/ATTRIBUTION.txt"
    assert p.expected == REPO / "target/xt-wiki/expected.json"
    assert p.embedder == REPO / "reference/models/all-MiniLM-L6-v2-q8"
    assert p.reranker == REPO / "reference/models/ms-marco-MiniLM-L-6-v2-q8"
    assert p.snapshot == REPO / "reference/datasets/wiki/simple.jsonl"
    assert p.manifest == REPO / "reference/datasets/wiki-manifest.json"
    assert p.queries == REPO / "reference/fixtures/008/queries.json"
    assert p.chonky == REPO / "reference/models/chonky_distilbert_base_uncased_1"


def test_flag_beats_env_beats_default(monkeypatch, tmp_path):
    monkeypatch.setenv("XTRIEVER_WIKI_ARTEFACT", str(tmp_path / "env-artefact"))
    monkeypatch.setenv("XTRIEVER_MODEL_DIR", str(tmp_path / "env-embedder"))
    monkeypatch.setenv("XTRIEVER_RERANK_MODEL_DIR", str(tmp_path / "env-reranker"))
    monkeypatch.setenv("XTRIEVER_CHONKY_MODEL_DIR", str(tmp_path / "env-chonky"))
    p = resolve(_args())
    assert p.chonky == tmp_path / "env-chonky"
    assert resolve(_args(chonky="rel/chonky")).chonky == REPO / "rel/chonky"
    assert p.artefact == tmp_path / "env-artefact"
    assert p.embedder == tmp_path / "env-embedder"
    assert p.reranker == tmp_path / "env-reranker"
    p = resolve(_args(artefact=str(tmp_path / "flag"), embedder="rel/embedder", reranker="/abs/reranker"))
    assert p.artefact == tmp_path / "flag"
    assert p.embedder == REPO / "rel/embedder"  # relative flags resolve against the root too
    assert str(p.reranker) == "/abs/reranker"
    assert p.expected == tmp_path / "flag/expected.json"  # follows the artefact


def test_missing_input_names_the_producer(tmp_path):
    p = resolve(_args(artefact=str(tmp_path / "none"), embedder=str(tmp_path / "e"), reranker=str(tmp_path / "r"), snapshot=str(tmp_path / "s.jsonl"), queries=str(tmp_path / "q.json")))
    assert first_missing(p, ["artefact"]) == ("the Wikipedia artefact", p.index_dir / "xtriever-pipeline.json", PRODUCERS["artefact"])
    # Feature 026: an absent model directory is named with both weights forms — never the float
    # file alone, which the eight-bit fetch it recommends does not create.
    assert first_missing(p, ["embedder"]) == ("the embedder", p.embedder / "{model.safetensors,*.gguf}", "scripts/fetch-model.sh --manifest reference/models/manifest-q8.json")
    assert first_missing(p, ["reranker"]) == ("the re-ranker", p.reranker / "{model.safetensors,*.gguf}", "scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json")
    assert first_missing(p, ["snapshot"]) == ("the snapshot", p.snapshot, "scripts/fetch-wiki.sh")
    assert first_missing(p, ["expected"])[2].startswith("cargo run --release -p xtriever-cli -- wiki expected")
    assert first_missing(p, ["queries"])[0] == "the measurement queries"
    q = resolve(_args(chonky=str(tmp_path / "c")))
    assert first_missing(q, ["chonky"]) == ("the chonky splitter model", q.chonky / "model.safetensors", "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json")
    assert "wiki build" in PRODUCERS["artefact"] and "wikidemo build" in PRODUCERS["artefact"]
    # In order: the first missing one is reported.
    assert first_missing(p, ["embedder", "artefact"])[0] == "the embedder"
    assert first_missing(resolve(_args(manifest=None)), ["manifest"]) is None
    try:
        require(p, ["artefact"])
    except MissingInput as e:
        assert str(e).startswith("missing the Wikipedia artefact: ")
        assert e.producer == PRODUCERS["artefact"]
    else:
        raise AssertionError("require() did not raise")


def test_a_model_directory_holds_exactly_one_weights_file(tmp_path):
    """Feature 026: the engine loads a directory holding the float weights or one GGUF and
    refuses one holding both, so the preflight refuses it too — with what is wrong, not with a
    command that would not mend it, and before anything loads."""
    both = tmp_path / "both"
    both.mkdir()
    (both / "model.safetensors").write_bytes(b"")
    (both / "all-MiniLM-L6-v2.Q8_0.gguf").write_bytes(b"")
    assert weights(both) is None, "neither file is the one the engine would load"
    p = resolve(_args(embedder=str(both), reranker=str(both)))
    assert first_missing(p, ["embedder"]) is None, "present, so not a missing input"
    try:
        require(p, ["embedder"])
    except UnusableInput as e:
        assert str(e).startswith("the embedder is unusable: ")
        assert "model.safetensors" in e.why and "all-MiniLM-L6-v2.Q8_0.gguf" in e.why
        assert "exactly one" in e.why
    else:
        raise AssertionError("require() did not raise")
    # One of the two is what the engine loads, and the preflight names it.
    (both / "model.safetensors").unlink()
    assert weights(both) == both / "all-MiniLM-L6-v2.Q8_0.gguf"
    assert require(resolve(_args(embedder=str(both))), ["embedder"]) is None


def test_cli_exits_1_on_an_unusable_model_directory(tmp_path):
    both = tmp_path / "both"
    both.mkdir()
    (both / "model.safetensors").write_bytes(b"")
    (both / "model.Q8_0.gguf").write_bytes(b"")
    # An artefact that is there, so the embedder is the first input with a problem.
    (tmp_path / "artefact/index").mkdir(parents=True)
    (tmp_path / "artefact/index/xtriever-pipeline.json").write_text("{}", encoding="utf-8")
    t = time.perf_counter()
    code, out, err = run_cli(["about", "--artefact", str(tmp_path / "artefact"), "--embedder", str(both)])
    assert code == 1
    assert err.startswith("wikidemo: the embedder is unusable: ")
    assert "holds exactly one weights file\n" in err
    assert out == ""
    assert time.perf_counter() - t < 1.0  # nothing was loaded


def test_cli_exits_1_on_a_missing_artefact():
    t = time.perf_counter()
    code, out, err = run_cli(["about", "--artefact", "/nonexistent"])
    assert code == 1
    assert err.startswith("wikidemo: missing the Wikipedia artefact: /nonexistent/index/xtriever-pipeline.json\n  produce it with: ")
    assert out == ""
    assert time.perf_counter() - t < 1.0  # nothing was loaded
