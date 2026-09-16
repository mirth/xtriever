"""``wikidemo build`` turns a (tiny, synthetic) snapshot into an 008-shaped artefact through
the package alone (spec FR-010–FR-013): the sidecar, the attribution, the build record, a
searchable index; and its refusals — an existing output, a snapshot that fails its hash, a
limit of zero, an article whose URL is not the derived one."""

import hashlib
import json

import pytest

from conftest import EMBEDDER, RERANKER, run_cli
from wikidemo.hits import wikipedia_url
from wikidemo.record import CHUNKER, corpus_identity

pytestmark = pytest.mark.models

ARTICLES = [
    {"id": "2004", "title": "Sky", "text": "The sky is the appearance of the atmosphere around the surface of the planet.\n\nThe sky is blue because of the random scattering of sunlight by the molecules."},
    {"id": "9001", "title": "Mercury (disambiguation)", "text": "Mercury may refer to: the planet, the element, the god."},
    {"id": "77", "title": "Café culture", "text": "A café is a place that sells coffee.\n\nPeople sit and talk in cafés for hours."},
]


def _write_snapshot(tmp_path, articles=ARTICLES, sha_override=None):
    lines = [json.dumps({**a, "url": wikipedia_url(a["title"])}, ensure_ascii=False) for a in articles]
    jsonl = tmp_path / "simple.jsonl"
    jsonl.write_text("\n".join(lines) + "\n", encoding="utf-8")
    raw = jsonl.read_bytes()
    manifest = {
        "edition": "simple",
        "snapshot_date": "2023-11-01",
        "source": "https://huggingface.co/datasets/wikimedia/wikipedia",
        "licence": {"name": "CC BY-SA 4.0", "url": "https://creativecommons.org/licenses/by-sa/4.0/"},
        "parquet": {"url": "x", "bytes": 1, "sha256": "p" * 64, "rows": len(articles)},
        "jsonl": {"file": "simple.jsonl", "bytes": len(raw), "sha256": sha_override or hashlib.sha256(raw).hexdigest(), "lines": len(articles)},
        "exclusions": [
            {"rule": "title_suffix", "value": " (disambiguation)"},
            {"rule": "lead_contains", "value": "may refer to", "within_chars": 300},
            {"rule": "lead_contains", "value": "may mean", "within_chars": 300},
        ],
    }
    mpath = tmp_path / "manifest.json"
    mpath.write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    return jsonl, mpath, manifest


ENV = {"XTRIEVER_MODEL_DIR": str(EMBEDDER), "XTRIEVER_RERANK_MODEL_DIR": str(RERANKER)}


def _build(tmp_path, out, extra=()):
    jsonl, mpath, manifest = _write_snapshot(tmp_path)
    code, stdout, stderr = run_cli(["build", "--out", str(out), "--snapshot", str(jsonl), "--manifest", str(mpath), *extra], ENV)
    return code, stdout, stderr, manifest


@pytest.fixture(scope="module")
def built(tmp_path_factory):
    tmp = tmp_path_factory.mktemp("build")
    out = tmp / "out"
    code, stdout, stderr, manifest = _build(tmp, out, ["--limit", "3"])
    assert code == 0, stderr
    return out, stdout, manifest


def test_build_produces_the_artefact(built):
    out, stdout, manifest = built
    assert (out / "index/xtriever-pipeline.json").exists()
    assert not out.with_name("out.partial").exists()
    sidecar = json.loads((out / "index/corpus.json").read_text(encoding="utf-8"))
    assert sidecar["schema_version"] == 1
    assert sidecar["partial"] == 3
    assert sidecar["snapshot"] == {
        "edition": "simple",
        "snapshot_date": "2023-11-01",
        "parquet_sha256": "p" * 64,
        "jsonl_sha256": manifest["jsonl"]["sha256"],
    }
    assert sidecar["exclusions"] == manifest["exclusions"]
    assert sidecar["chunker"] == CHUNKER
    assert sidecar["embedder_fingerprint"].startswith("sentence-transformers/all-MiniLM-L6-v2@")
    counts = sidecar["counts"]
    assert counts["articles"] == 3 and counts["selected"] == 2
    assert counts["excluded"] == {"title_suffix: (disambiguation)": 1, "lead_contains:may refer to:300": 0, "lead_contains:may mean:300": 0}
    assert counts["passages"] >= 2 and counts["url_mismatches"] == 0 and counts["passages_over_window"] == 0
    attribution = (out / "ATTRIBUTION.txt").read_text(encoding="utf-8")
    lines = attribution.splitlines()
    assert len(lines) == 4 and lines[0].startswith("Text from Simple English Wikipedia, snapshot 2023-11-01 (")
    assert f"Corpus identity {sidecar['corpus_identity']}, built " in lines[3]
    record = json.loads((out / "wiki-build.json").read_text(encoding="utf-8"))
    assert set(record) == {"schema_version", "feature", "recorded_at", "corpus_identity", "partial", "host", "models", "counts", "phases_ms", "artefact_bytes"}
    assert record["feature"] == "019-python-wiki-demo" and record["corpus_identity"] == sidecar["corpus_identity"]
    assert set(record["phases_ms"]) == {"fetch_verify", "read_exclude", "chunk", "embed_ingest", "commit", "merge", "total"}
    assert record["artefact_bytes"]["total"] > 0 and record["counts"] == counts
    assert "wrote " in stdout and "corpus identity: " in stdout


def test_identity_matches_the_record_helper(built):
    out, _, manifest = built
    sidecar = json.loads((out / "index/corpus.json").read_text(encoding="utf-8"))
    assert sidecar["corpus_identity"] == corpus_identity(sidecar["snapshot"], sidecar["exclusions"], CHUNKER, sidecar["embedder_fingerprint"], partial=3)


def test_built_index_searches(built):
    out, _, _ = built
    code, stdout, stderr = run_cli(["search", "--artefact", str(out), "-k", "2", "--depth", "5", "scattering of sunlight"], ENV)
    assert code == 0, stderr
    assert "Sky  2004#" in stdout and "https://simple.wikipedia.org/wiki/Sky" in stdout
    assert "Mercury" not in stdout
    code, stdout, stderr = run_cli(["about", "--artefact", str(out)], ENV)
    assert code == 0 and "partial: first 3 articles" in stdout and "articles: 3 read, 2 selected" in stdout


def test_refuses_existing_out(built, tmp_path):
    out, _, _ = built
    code, stdout, stderr = _build(tmp_path, out, ["--limit", "1"])[:3]
    assert code == 1 and "already exists" in stderr


def test_snapshot_hash_mismatch_refuses(tmp_path):
    jsonl, mpath, _ = _write_snapshot(tmp_path, sha_override="0" * 64)
    code, stdout, stderr = run_cli(["build", "--out", str(tmp_path / "out"), "--snapshot", str(jsonl), "--manifest", str(mpath), "--limit", "1"], ENV)
    assert code == 1
    assert "0" * 64 in stderr and "scripts/fetch-wiki.sh" in stderr
    assert not (tmp_path / "out").exists()


def test_limit_zero_is_a_usage_error(tmp_path):
    code, stdout, stderr = run_cli(["build", "--out", str(tmp_path / "out"), "--limit", "0"], ENV)
    assert code == 2


def test_url_mismatch_is_an_error(tmp_path):
    jsonl, mpath, _ = _write_snapshot(tmp_path)
    bad = json.dumps({"id": "5", "title": "Odd", "url": "https://simple.wikipedia.org/wiki/Other", "text": "Body."})
    jsonl.write_text(bad + "\n", encoding="utf-8")
    raw = jsonl.read_bytes()
    manifest = json.loads(mpath.read_text(encoding="utf-8"))
    manifest["jsonl"].update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(), lines=1)
    mpath.write_text(json.dumps(manifest), encoding="utf-8")
    code, stdout, stderr = run_cli(["build", "--out", str(tmp_path / "out"), "--snapshot", str(jsonl), "--manifest", str(mpath), "--limit", "1"], ENV)
    assert code == 1 and "article 5" in stderr and "Odd" in stderr
    assert not (tmp_path / "out").exists()
