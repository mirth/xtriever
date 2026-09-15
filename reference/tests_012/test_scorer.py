"""SC-001: the engine's exported runs, re-scored by the 003 reference, reproduce the committed
baselines to 1e-6 — the two scorers agree before any variant counts. Red until `export` exists."""

import json

import pytest

import gen_003_fixtures as ref  # noqa: E402  (sys.path from conftest)
from conftest import REPO

TOL = 1e-6
BASELINES = {
    "lexical": "specs/003-eval-harness/baselines/lexical-baseline-v1.{d}.json",
    "dense": "specs/004-dense-stage/baselines/dense-baseline-v1.{d}.json",
    "hybrid": "specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.{d}.json",
}


def test_probe():
    ref.probe()


def load_run(path):
    run = {}
    for line in path.read_text().splitlines():
        if line.strip():
            rec = json.loads(line)
            run[rec["query_id"]] = rec["doc_ids"]
    return run


@pytest.mark.parametrize("dataset", ["scifact", "nfcorpus", "fiqa"])
@pytest.mark.parametrize("stage", ["lexical", "dense", "hybrid"])
def test_engine_runs_reproduce_the_baselines(dataset, stage):
    run_path = REPO / "target" / "xt-sparse-runs" / dataset / f"engine-{stage}.jsonl"
    assert run_path.exists(), f"missing {run_path} — run `sparse_spike.py export --dataset {dataset}`"
    qrels = ref.load_qrels_tsv(REPO / "reference" / "datasets" / "beir" / dataset / "qrels" / "test.tsv")
    got = ref.reference(qrels, load_run(run_path))
    want = json.loads((REPO / BASELINES[stage].format(d=dataset)).read_text())
    assert got["scored_queries"] == want["scored_queries"]
    assert abs(got["mean_ndcg_10"] - want["mean_ndcg_10"]) <= TOL, (got["mean_ndcg_10"], want["mean_ndcg_10"])
    assert abs(got["mean_recall_100"] - want["mean_recall_100"]) <= TOL
