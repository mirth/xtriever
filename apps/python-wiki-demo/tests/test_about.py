"""About prints the engine's information and the corpus sidecar, and the attribution verbatim
(spec FR-015, SC-003). Against the Wikipedia artefact when it is on disk, else the fixture
artefact (no sidecar: those lines say so)."""

import json
from types import SimpleNamespace

import pytest

from conftest import EMBEDDER, RERANKER, WIKI_ARTEFACT, run_cli
from wikidemo import DEFAULT_DEPTH
from wikidemo.about import about_lines, read_attribution, read_sidecar
from wikidemo.inputs import resolve
from wikidemo.search import open_artefact

pytestmark = pytest.mark.models


@pytest.fixture(scope="module")
def artefact(fixture_artefact):
    return WIKI_ARTEFACT if (WIKI_ARTEFACT / "index/xtriever-pipeline.json").exists() else fixture_artefact


def _paths(artefact):
    return resolve(SimpleNamespace(artefact=str(artefact), embedder=str(EMBEDDER), reranker=str(RERANKER), snapshot=None, manifest=None, expected=None, queries=None))


def test_about_fields_equal_info_and_sidecar(artefact):
    paths = _paths(artefact)
    opened = open_artefact(paths)
    sidecar = read_sidecar(paths)
    lines = about_lines(opened, sidecar, read_attribution(paths), "https://creativecommons.org/licenses/by-sa/4.0/")
    text = "\n".join(lines)
    info = opened.info
    assert f"passages (documents): {info.documents:,}" in text
    assert f"format version: {info.format_version}" in text
    assert f"embedder: {info.embedder_fingerprint}" in text
    assert f"re-ranker: {info.reranker_model_id}" in text
    assert f"candidate depth: {info.candidate_depth}" in text and f"rrf k: {info.rrf_k}" in text
    i = lines.index(f"re-rank depth (engine default): {info.rerank_depth}")
    assert lines[i + 1] == f"re-rank depth (demo default): {DEFAULT_DEPTH}"
    assert "re-rank mode: interpolate α 0.5" in text
    assert f"open: {opened.open_ms} ms" in text and f"embedder load: {info.embedder_load_ms} ms" in text
    if sidecar is None:
        assert "corpus identity: (no corpus sidecar)" in text
    else:
        raw = json.loads((paths.corpus_json).read_text(encoding="utf-8"))
        assert f"corpus identity: {raw['corpus_identity']}" in text
        assert f"corpus: Simple English Wikipedia ({raw['snapshot']['edition']}), snapshot {raw['snapshot']['snapshot_date']}" in text
        assert f"articles: {raw['counts']['articles']:,} read, {raw['counts']['selected']:,} selected, {raw['counts']['passages']:,} passages" in text
        assert ("partial:" in text) == ("partial" in raw)
    assert lines[-1] == "licence: https://creativecommons.org/licenses/by-sa/4.0/"


def test_attribution_verbatim(artefact):
    env = {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)}
    code, out, err = run_cli(["about", "--artefact", str(artefact)], env)
    assert code == 0, err
    attribution = (artefact / "ATTRIBUTION.txt").read_text(encoding="utf-8")
    assert "\n\n" + attribution + "licence: " in out
