#!/usr/bin/env python3
"""Generate the metric goldens for Feature 003 (the evaluation harness).

The reference implementation is ``pytrec_eval`` — the ``trec_eval`` binding that BEIR's own
``EvaluateRetrieval.evaluate`` uses — so every golden here, every published BEIR number and the
Rust metrics share one definition of nDCG@10 and Recall@100 (research D3).

Two jobs:

* default: emit ``metrics.json`` (eleven synthetic cases, data-model ``MetricGoldens``) and a
  ``manifest.json`` with a SHA-256 per file. Before writing anything the script re-runs the
  convention probe from research D3 and **refuses** if ``pytrec_eval`` behaves differently.
* ``--verify-run``: score a run exported by the Rust harness under BEIR's exact wrapper semantics
  and compare against the Rust report (SC-001 at scale).

Run via the pinned virtualenv:

    scripts/setup-reference-venv.sh 003
    reference/.venv-003/bin/python reference/gen_003_fixtures.py --seed 3
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import sys
from pathlib import Path

_REQUIRED_PY = (3, 12)
if sys.version_info[:2] != _REQUIRED_PY or sys.prefix == sys.base_prefix:
    sys.exit(
        f"gen_003_fixtures.py requires the pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh 003\n"
        "  reference/.venv-003/bin/python reference/gen_003_fixtures.py --seed 3"
    )

import pytrec_eval  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "reference" / "fixtures" / "003"
TOL = 1e-6  # spec.md Assumptions: metric tolerance, absolute
MEASURES = {"ndcg_cut.10", "recall.100"}
K = 100  # retrieval depth; scores handed to pytrec_eval are K - rank so list order is preserved


# --------------------------------------------------------------------------------------------
# Reference scoring under BEIR's wrapper semantics (beir/retrieval/evaluation.py, read 2026-09-12):
#   * results whose doc id equals the query id are popped before scoring (ignore_identical_ids)
#   * pytrec_eval scores only queries present in BOTH the run and the qrels
#   * the mean is over the queries pytrec_eval returned, then round(., 5) for presentation
# --------------------------------------------------------------------------------------------
def run_scores(run: dict[str, list[str]]) -> dict[str, dict[str, float]]:
    """Ordered id lists -> trec-style score dicts that reproduce the list order (research D4)."""
    out = {}
    for qid, ids in run.items():
        seen: dict[str, float] = {}
        for rank, did in enumerate(ids):
            if did not in seen:  # a repeated id keeps its first rank
                seen[did] = float(K - rank)
        out[qid] = seen
    return out


def reference(qrels: dict[str, dict[str, int]], run: dict[str, list[str]]) -> dict:
    scored_run = run_scores(run)
    dropped = 0
    for qid in scored_run:
        if qid in scored_run[qid]:
            scored_run[qid].pop(qid)
            dropped += 1
    ev = pytrec_eval.RelevanceEvaluator(qrels, MEASURES)
    scores = ev.evaluate(scored_run)
    per_query = {q: {"ndcg_10": v["ndcg_cut_10"], "recall_100": v["recall_100"]} for q, v in scores.items()}
    n = len(scores)
    mean_ndcg = sum(per_query[q]["ndcg_10"] for q in sorted(per_query)) / n if n else 0.0
    mean_rec = sum(per_query[q]["recall_100"] for q in sorted(per_query)) / n if n else 0.0
    return {
        "per_query": per_query,
        "scored_queries": n,
        "dropped_identical": dropped,
        "mean_ndcg_10": mean_ndcg,
        "mean_recall_100": mean_rec,
        "beir_rounded": {"ndcg_10": round(mean_ndcg, 5), "recall_100": round(mean_rec, 5)},
    }


# --------------------------------------------------------------------------------------------
# Convention probe (research D3). If any of these stops holding, the goldens would silently
# encode a different metric — so refuse instead.
# --------------------------------------------------------------------------------------------
def probe() -> None:
    qrels = {
        "q_graded": {"d1": 2, "d2": 1, "d3": 0},
        "q_norel": {"d9": 0},
        "q_absent_from_run": {"d1": 1},
        "q_few": {"d1": 1, "d2": 1, "d3": 1},
    }
    run = {
        "q_graded": {"d2": 3.0, "d1": 2.0, "d3": 1.0, "d4": 0.5},
        "q_norel": {"d1": 1.0},
        "q_unjudged": {"d1": 1.0},
        "q_few": {"d2": 1.0},
        "q_empty": {},
    }
    ev = pytrec_eval.RelevanceEvaluator(qrels, MEASURES)
    res = ev.evaluate(run)
    checks = {
        "linear gain (d2 rank1 grade1, d1 rank2 grade2)": abs(res["q_graded"]["ndcg_cut_10"] - 0.8597186998521972) < 1e-12,
        "run query absent from qrels is ignored": "q_unjudged" not in res and "q_empty" not in res,
        "judged query absent from run is not returned": "q_absent_from_run" not in res,
        "judged query with no relevant doc scores 0.0": res["q_norel"] == {"recall_100": 0.0, "ndcg_cut_10": 0.0},
        "fewer results than cutoff": abs(res["q_few"]["recall_100"] - 1 / 3) < 1e-12,
        "judged query with empty results scores 0.0": ev.evaluate({"q_few": {}})["q_few"]["ndcg_cut_10"] == 0.0,
        "ties ordered by doc id descending (d9, d2, d1)": abs(
            ev.evaluate({"q_few": {"d1": 1.0, "d9": 1.0, "d2": 1.0}})["q_few"]["ndcg_cut_10"]
            - (1 / math.log2(3) + 1 / math.log2(4)) / (1 + 1 / math.log2(3) + 1 / math.log2(4))
        ) < 1e-12,
    }
    failed = [name for name, ok in checks.items() if not ok]
    if failed:
        sys.exit("gen_003_fixtures: REFUSING — pytrec_eval no longer matches research D3:\n  " + "\n  ".join(failed))
    print(f"gen_003_fixtures: convention probe OK ({len(checks)} checks, pytrec_eval {pytrec_eval.__version__ if hasattr(pytrec_eval, '__version__') else '0.5'})")


# --------------------------------------------------------------------------------------------
# Golden cases (data-model `MetricGoldens`).
# --------------------------------------------------------------------------------------------
def gen_cases(seed: int) -> list[dict]:
    rng = random.Random(seed)
    cases = []

    def case(name, qrels, run, note):
        cases.append({"name": name, "note": note, "qrels": qrels, "run": run, "expected": reference(qrels, run)})

    case("graded", {"q1": {"d1": 2, "d2": 1, "d3": 0}}, {"q1": ["d2", "d1", "d3", "d4"]},
         "linear gain: grade 2 at rank 2, grade 1 at rank 1 (NFCorpus has grade 2)")
    case("no_relevant", {"q1": {"d9": 0}, "q2": {"d1": 1}}, {"q1": ["d1", "d2"], "q2": ["d1"]},
         "q1 is judged but has no relevant document: scored 0.0 and counted in the mean")
    case("fewer_than_cutoff", {"q1": {"d1": 1, "d2": 1, "d3": 1}}, {"q1": ["d2", "d7", "d1"]},
         "3 results, 3 relevant, 2 retrieved: recall 2/3, ndcg over what was returned")
    case("empty_results", {"q1": {"d1": 1}, "q2": {"d1": 1}}, {"q1": [], "q2": ["d1"]},
         "judged query with an empty list scores 0.0 and is counted")
    case("unjudged_query", {"q1": {"d1": 1}}, {"q1": ["d1"], "q_extra": ["d1", "d2"]},
         "q_extra is in the run but not in the qrels: ignored, not in the mean")
    case("not_retrieved", {"q1": {"d1": 1}, "q2": {"d5": 1}}, {"q1": ["d1"]},
         "q2 is judged but absent from the run: excluded from the mean (BEIR means over returned keys)")
    case("duplicate_ids", {"q1": {"d1": 1, "d2": 1}}, {"q1": ["d3", "d1", "d1", "d1", "d2"]},
         "a repeated id keeps its first rank and counts once")
    long_run = [f"d{i}" for i in range(1, 13)]
    case("ties_at_cutoff", {"q1": {"d10": 1, "d11": 1, "d12": 1, "d100": 1}}, {"q1": long_run + [f"x{i}" for i in range(13, 100)] + ["d100", "d101"]},
         "the order given is the order scored: d10 inside the nDCG cutoff, d11 just outside; d100 at rank 100 inside recall@100, d101 outside")
    case("identical_ids", {"7": {"7": 1, "d1": 1}}, {"7": ["7", "d1", "d2"]},
         "a result whose id equals the query id is dropped before scoring (BEIR ignore_identical_ids); FiQA has 55 such id pairs")
    case("identical_ids_repeated", {"7": {"7": 1, "d1": 1}, "8": {"d1": 1}}, {"7": ["7", "d1", "7", "7"], "8": ["d1"]},
         "BEIR converts a run to a doc->score map first, so a self id repeated three times is popped and counted ONCE")
    case("grade_zero", {"q1": {"d1": 0, "d2": 1}}, {"q1": ["d1", "d2"]},
         "an explicit 0 grade is non-relevant and not in the recall denominator: recall 1.0, ndcg = 1/log2(3)")

    # big_mean: 50 queries, varied sizes, mean in fixed order
    qrels, run = {}, {}
    pool = [f"doc{i}" for i in range(400)]
    for i in range(50):
        qid = f"q{i:02d}"
        rel = rng.sample(pool, rng.randint(1, 8))
        qrels[qid] = {d: rng.choice([1, 1, 1, 2]) for d in rel}
        if rng.random() < 0.1:
            qrels[qid][rng.choice(pool)] = 0
        n = rng.randint(0, 100)
        ranked = rng.sample(pool, n)
        run[qid] = ranked
    case("big_mean", qrels, run, "50 seeded queries; the mean is summed in ascending query-id order")
    return cases


# --------------------------------------------------------------------------------------------
# --verify-run: the Rust harness exports {"query_id", "doc_ids"} per line; score it like BEIR would.
# --------------------------------------------------------------------------------------------
def load_qrels_tsv(path: Path) -> dict[str, dict[str, int]]:
    qrels: dict[str, dict[str, int]] = {}
    with path.open("rb") as f:
        lines = f.read().decode("utf-8").split("\n")
    for i, line in enumerate(lines):
        line = line.rstrip("\r")  # research D2: CRLF with a header line
        if not line or i == 0:
            continue
        q, d, s = line.split("\t")
        qrels.setdefault(q, {})[d] = int(s)
    return qrels


def verify_run(run_path: Path, qrels_path: Path, report_path: Path) -> int:
    run: dict[str, list[str]] = {}
    for line in run_path.read_text().splitlines():
        if line.strip():
            rec = json.loads(line)
            run[rec["query_id"]] = rec["doc_ids"]
    qrels = load_qrels_tsv(qrels_path)
    ref = reference(qrels, run)
    report = json.loads(report_path.read_text())
    ok = True
    for key in ("mean_ndcg_10", "mean_recall_100"):
        d = abs(ref[key] - report[key])
        flag = "ok" if d <= TOL else "MISMATCH"
        ok &= d <= TOL
        print(f"  {key}: python {ref[key]:.10f} rust {report[key]:.10f} |Δ|={d:.2e} {flag}")
    for key in ("ndcg_10", "recall_100"):
        same = ref["beir_rounded"][key] == report["beir_rounded"][key]
        ok &= same
        print(f"  beir_rounded.{key}: python {ref['beir_rounded'][key]} rust {report['beir_rounded'][key]} {'ok' if same else 'MISMATCH'}")
    for key in ("scored_queries", "dropped_identical"):
        same = ref[key] == report[key]
        ok &= same
        print(f"  {key}: python {ref[key]} rust {report[key]} {'ok' if same else 'MISMATCH'}")
    print(f"verify-run: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


# --------------------------------------------------------------------------------------------
def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path: Path, payload: object) -> None:
    path.write_text(json.dumps(payload, indent=1, ensure_ascii=True) + "\n")
    print(f"  wrote {path.relative_to(REPO_ROOT) if path.is_relative_to(REPO_ROOT) else path}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=3)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--verify-run", type=Path, metavar="RUN_JSONL")
    ap.add_argument("--qrels", type=Path)
    ap.add_argument("--report", type=Path)
    args = ap.parse_args()

    probe()
    if args.verify_run:
        if not (args.qrels and args.report):
            sys.exit("--verify-run needs --qrels and --report")
        return verify_run(args.verify_run, args.qrels, args.report)

    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    cases = gen_cases(args.seed)
    write_json(out / "metrics.json", {"tolerance": TOL, "k": K, "reference": "pytrec_eval 0.5 under BEIR's evaluate() semantics", "cases": cases})
    write_json(out / "manifest.json", {"generator": "gen_003_fixtures.py", "seed": args.seed, "files": {"metrics.json": sha256_file(out / "metrics.json")}})
    print(f"gen_003_fixtures: done ({len(cases)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
