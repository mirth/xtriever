#!/usr/bin/env python3
"""Feature 014 — the re-rank depth study (specs/014-rerank-depth-study).

Derives every re-rank variant × depth from one depth-50 explain export per dataset
(`beir run --config hybrid-rerank-v2 --rerank-depth 50 --export-explain …`), scores each
derived run with the 003 reference, checks the derivation against end-to-end runs, tabulates
the cells and applies the decision rule fixed in the spec (research D4, D5, D7).

The derivation is exact because the pinned re-ranker scores every (query, passage) pair alone
(crates/xtriever-rerank/src/scorer.rs, research D2): a pair's score at depth 50 is its score
at any depth. `check` verifies that instead of assuming it.

Subcommands (contracts/study-cli.md): derive | score | check | table | decide | all.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "reference"))

import gen_003_fixtures as ref003  # noqa: E402

K = 100
RRF_K = 60
DEPTHS = [5, 10, 20, 50]
DATASETS = ["scifact", "nfcorpus", "fiqa"]
RUNS_DIR = REPO / "specs" / "014-rerank-depth-study" / "runs"
STUDY_DIR = REPO / "target" / "xt-rerank-study"
DEPTH0_BASELINE = REPO / "specs" / "013-lexical-quality" / "baselines" / "hybrid-baseline-v2.{d}.json"
DEFAULT = ("replace", 20)
DEFAULT_MEAN = 0.4768  # hybrid-rerank-v2's three-set mean (013 report)
MEAN_FLOOR = DEFAULT_MEAN + 0.005
MAX_DROP = 0.005
OLD_EXPLAIN = "explain export predates 014; re-run with the current `beir`"


# ── reading ──────────────────────────────────────────────────────────────────────────────────
def read_explain(path: Path) -> list[dict]:
    lines = []
    for raw in Path(path).read_text().splitlines():
        if not raw.strip():
            continue
        rec = json.loads(raw)
        if "fused_scores" not in rec:
            raise ValueError(OLD_EXPLAIN)
        lines.append(rec)
    return lines


def corpus_positions(dataset: str) -> dict[str, int]:
    """External id → corpus position: the harness's `DocId(i)` (eval `build`), the tie key."""
    positions: dict[str, int] = {}
    with (REPO / "reference" / "datasets" / "beir" / dataset / "corpus.jsonl").open("rb") as f:
        for i, raw in enumerate(f):
            positions[json.loads(raw)["_id"]] = i
    return positions


def read_run(path: Path) -> dict[str, list[str]]:
    run = {}
    for raw in Path(path).read_text().splitlines():
        if raw.strip():
            rec = json.loads(raw)
            run[rec["query_id"]] = rec["doc_ids"]
    return run


def write_run(path: Path, run: dict[str, list[str]]) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as f:
        for qid in sorted(run):
            f.write(json.dumps({"query_id": qid, "doc_ids": run[qid]}) + "\n")


# ── the head and the variants (research D4) ──────────────────────────────────────────────────
def head(line: dict, d: int, positions: dict[str, int]) -> list[dict]:
    """The first `d` fused candidates that carry a cross-encoder score, with their fused rank
    `r_f` (1-based), fused score `s_f`, cross-encoder score `s_c` and position `pos`."""
    scores = {i: s for _, i, s in line["rerank"]}
    out = []
    for r, (i, s_f) in enumerate(zip(line["fused"][:d], line["fused_scores"][:d])):
        if i in scores:
            out.append({"id": i, "pos": positions[i], "r_f": r + 1, "s_f": s_f, "s_c": scores[i]})
    return out


def rest(line: dict, head_entries: list[dict]) -> list[str]:
    head_ids = {h["id"] for h in head_entries}
    return [i for i in line["fused"] if i not in head_ids]


def order_replace(head_entries: list[dict], tail: list[str], k: int) -> list[str]:
    """The pipeline's rule (`xtriever_pipeline::rerank::order_reranked`): the head by
    (−cross-encoder score, position), then the rest in fused order, cut at k."""
    ordered = sorted(head_entries, key=lambda h: (-h["s_c"], h["pos"]))
    return ([h["id"] for h in ordered] + tail)[:k]


def order_rrf(head_entries: list[dict], tail: list[str], k: int) -> list[str]:
    """Rank fusion within the head: 1/(60 + r_f) + 1/(60 + r_c), r_c the 1-based rank by
    (−cross-encoder score, position); ties by position."""
    by_ce = sorted(head_entries, key=lambda h: (-h["s_c"], h["pos"]))
    r_c = {h["id"]: r + 1 for r, h in enumerate(by_ce)}
    score = {h["id"]: 1.0 / (RRF_K + h["r_f"]) + 1.0 / (RRF_K + r_c[h["id"]]) for h in head_entries}
    ordered = sorted(head_entries, key=lambda h: (-score[h["id"]], h["pos"]))
    return ([h["id"] for h in ordered] + tail)[:k]


def minmax(values: list[float]) -> list[float]:
    """Per-column min-max to [0, 1]; a constant column (or a single value) is all zeros."""
    if not values:
        return []
    lo, hi = min(values), max(values)
    if hi == lo:
        return [0.0 for _ in values]
    return [(v - lo) / (hi - lo) for v in values]


def order_lin(head_entries: list[dict], tail: list[str], k: int, alpha: float) -> list[str]:
    """(1 − α)·minmax(fused) + α·minmax(cross-encoder) within the head; ties fall back to the
    fused order (r_f), which the engine already tie-broke by position."""
    f = minmax([h["s_f"] for h in head_entries])
    c = minmax([h["s_c"] for h in head_entries])
    s = {h["id"]: (1.0 - alpha) * f[i] + alpha * c[i] for i, h in enumerate(head_entries)}
    ordered = sorted(head_entries, key=lambda h: (-s[h["id"]], h["r_f"]))
    return ([h["id"] for h in ordered] + tail)[:k]


VARIANTS = {
    "replace": order_replace,
    "rrf": order_rrf,
    "lin-0.25": lambda h, t, k: order_lin(h, t, k, 0.25),
    "lin-0.5": lambda h, t, k: order_lin(h, t, k, 0.5),
    "lin-0.75": lambda h, t, k: order_lin(h, t, k, 0.75),
}


def derive(lines, positions, variants, depths, k=K) -> dict[tuple[str, int], dict[str, list[str]]]:
    runs: dict[tuple[str, int], dict[str, list[str]]] = {}
    for variant in variants:
        order = VARIANTS[variant]
        for d in depths:
            run = {}
            for line in lines:
                h = head(line, d, positions)
                run[line["query_id"]] = order(h, rest(line, h), k)
            runs[(variant, d)] = run
    return runs


# ── scoring, checking, tabulating, deciding ──────────────────────────────────────────────────
def score_run(qrels: dict, run: dict[str, list[str]]) -> dict:
    return ref003.reference(qrels, run)


def qrels_for(dataset: str) -> dict:
    return ref003.load_qrels_tsv(REPO / "reference" / "datasets" / "beir" / dataset / "qrels" / "test.tsv")


def cell_path(runs_dir: Path, variant: str, d: int, dataset: str) -> Path:
    return runs_dir / f"{variant}-d{d}.{dataset}.json"


def compare_runs(name: str, got: dict, want: dict) -> str | None:
    if set(got) != set(want):
        return f"{name}: query sets differ ({len(got)} vs {len(want)})"
    for qid in sorted(want):
        if got[qid] != want[qid]:
            return f"{name}: query {qid} differs: got {got[qid][:8]} want {want[qid][:8]}"
    return None


def per_query_pair(entry) -> tuple[float, float]:
    """The harness report stores `[ndcg, recall]` per query; the 003 scorer a dict."""
    if isinstance(entry, dict):
        return entry["ndcg_10"], entry["recall_100"]
    return entry[0], entry[1]


def compare_metrics(name: str, got: dict, want: dict, tol: float = 1e-6) -> str | None:
    if set(got["per_query"]) != set(want["per_query"]):
        return f"{name}: scored query sets differ"
    for qid in sorted(want["per_query"]):
        a, b = per_query_pair(got["per_query"][qid]), per_query_pair(want["per_query"][qid])
        for m, x, y in zip(("ndcg_10", "recall_100"), a, b):
            if abs(x - y) > tol:
                return f"{name}: query {qid} {m} {x} vs {y}"
    for m in ("mean_ndcg_10", "mean_recall_100"):
        if abs(got[m] - want[m]) > tol:
            return f"{name}: {m} {got[m]} vs {want[m]}"
    return None


def decide(rows: list[dict], depth0: dict[str, float], *, default_mean=DEFAULT_MEAN,
           mean_floor=MEAN_FLOOR, max_drop=MAX_DROP) -> dict:
    """Research D7: qualifies iff mean ≥ default + 0.005 and no dataset > 0.005 below depth 0;
    highest qualifying mean wins, ties by the smaller depth; none → the default stays."""
    qualifying = [
        r for r in rows
        if r["mean"] >= mean_floor - 1e-12
        and all(r["ndcg"][ds] >= depth0[ds] - max_drop - 1e-12 for ds in depth0)
    ]
    qualifying.sort(key=lambda r: (-r["mean"], r["depth"]))
    winner = qualifying[0] if qualifying else None
    statement = (
        f"{winner['variant']}-d{winner['depth']} replaces the default (mean {winner['mean']:.4f})"
        if winner
        else "no configuration qualifies; the default stays"
    )
    return {
        "rule": {"mean_floor": mean_floor, "max_drop": max_drop, "default": f"{DEFAULT[0]}-d{DEFAULT[1]}", "default_mean": default_mean},
        "qualifying": qualifying,
        "winner": winner,
        "statement": statement,
    }


def load_table_rows(runs_dir: Path) -> tuple[list[dict], dict[str, dict]]:
    depth0 = {}
    for ds in DATASETS:
        rep = json.loads(DEPTH0_BASELINE.with_name(DEPTH0_BASELINE.name.format(d=ds)).read_text())
        depth0[ds] = {"ndcg_10": rep["mean_ndcg_10"], "recall_100": rep["mean_recall_100"]}
    rows = []
    for variant in VARIANTS:
        for d in DEPTHS:
            cells = {}
            for ds in DATASETS:
                p = cell_path(runs_dir, variant, d, ds)
                if not p.exists():
                    break
                c = json.loads(p.read_text())
                cells[ds] = {"ndcg_10": c["ndcg_10"], "recall_100": c["recall_100"]}
            if len(cells) != len(DATASETS):
                continue
            rows.append({
                "variant": variant, "depth": d, "calls_per_query": d,
                "ndcg": {ds: cells[ds]["ndcg_10"] for ds in DATASETS},
                "recall": {ds: cells[ds]["recall_100"] for ds in DATASETS},
                "mean": sum(cells[ds]["ndcg_10"] for ds in DATASETS) / len(DATASETS),
            })
    return rows, depth0


def table_markdown(rows: list[dict], depth0: dict[str, dict]) -> str:
    default = next((r for r in rows if (r["variant"], r["depth"]) == DEFAULT), None)
    d0_mean = sum(depth0[ds]["ndcg_10"] for ds in DATASETS) / len(DATASETS)
    out = ["| variant | depth | " + " | ".join(DATASETS) + " | mean | Δ vs depth 0 | Δ vs replace-d20 | calls/query |",
           "|---|---|" + "---|" * len(DATASETS) + "---|---|---|---|"]
    out.append("| depth0 (hybrid-baseline-v2) | 0 | " + " | ".join(f"{depth0[ds]['ndcg_10']:.4f}" for ds in DATASETS)
               + f" | {d0_mean:.4f} | — | {d0_mean - default['mean']:+.4f} | 0 |" if default else " | — | — | 0 |")
    for r in rows:
        out.append(
            f"| {r['variant']} | {r['depth']} | "
            + " | ".join(f"{r['ndcg'][ds]:.4f} ({r['ndcg'][ds] - depth0[ds]['ndcg_10']:+.4f})" for ds in DATASETS)
            + f" | {r['mean']:.4f} | {r['mean'] - d0_mean:+.4f} | "
            + (f"{r['mean'] - default['mean']:+.4f}" if default else "—")
            + f" | {r['calls_per_query']} |"
        )
    return "\n".join(out) + "\n"


# ── commands ─────────────────────────────────────────────────────────────────────────────────
def cmd_derive(args) -> int:
    lines = read_explain(args.explain)
    positions = corpus_positions(args.dataset)
    depths = DEPTHS
    for line in lines:
        scored = len(line["rerank"])
        if scored < min(max(depths), len(line["fused"])):
            print(f"explain has only {scored} scored candidates for query {line['query_id']}; need {max(depths)}")
            return 2
    out_dir = Path(args.out_dir or STUDY_DIR / args.dataset)
    for (variant, d), run in derive(lines, positions, list(VARIANTS), depths).items():
        write_run(out_dir / f"{variant}-d{d}.jsonl", run)
    print(f"derived {len(VARIANTS) * len(depths)} runs into {out_dir}")
    return 0


def cmd_score(args) -> int:
    ref003.probe()
    qrels = qrels_for(args.dataset)
    runs_dir = Path(args.runs_dir or STUDY_DIR / args.dataset)
    out_dir = Path(args.out_dir or RUNS_DIR)
    out_dir.mkdir(parents=True, exist_ok=True)
    for variant in VARIANTS:
        for d in DEPTHS:
            run = read_run(runs_dir / f"{variant}-d{d}.jsonl")
            rep = score_run(qrels, run)
            cell = {
                "variant": variant, "depth": d, "dataset": args.dataset, "source": "derived",
                "ndcg_10": rep["mean_ndcg_10"], "recall_100": rep["mean_recall_100"],
                "beir_rounded": rep["beir_rounded"], "scored_queries": rep["scored_queries"],
                "calls_per_query": d, "per_query": rep["per_query"],
            }
            cell_path(out_dir, variant, d, args.dataset).write_text(json.dumps(cell, indent=1, sort_keys=True) + "\n")
            print(f"{args.dataset} {variant}-d{d}: nDCG@10={rep['mean_ndcg_10']:.6f} Recall@100={rep['mean_recall_100']:.6f}")
    return 0


def cmd_check(args) -> int:
    """Derived replace-d20 = the 013 run and report; replace-d<e2e> = an end-to-end run;
    replace-d50 = the explain's own hits; Recall@100 = depth 0 in every cell."""
    qrels = qrels_for(args.dataset)
    runs_dir = Path(args.runs_dir or STUDY_DIR / args.dataset)
    failures = []
    d20 = read_run(runs_dir / "replace-d20.jsonl")
    if err := compare_runs("replace-d20 vs 013 run", d20, read_run(args.baseline_run)):
        failures.append(err)
    if err := compare_metrics("replace-d20 vs 013 report", score_run(qrels, d20), json.loads(Path(args.baseline_report).read_text())):
        failures.append(err)
    if args.e2e_run:
        got = read_run(runs_dir / f"replace-d{args.e2e_depth}.jsonl")
        if err := compare_runs(f"replace-d{args.e2e_depth} vs end-to-end", got, read_run(args.e2e_run)):
            failures.append(err)
    if args.explain:
        hits = {l["query_id"]: l["hits"][:K] for l in read_explain(args.explain)}
        if err := compare_runs("replace-d50 vs explain hits", read_run(runs_dir / "replace-d50.jsonl"), hits):
            failures.append(err)
    depth0 = json.loads(DEPTH0_BASELINE.with_name(DEPTH0_BASELINE.name.format(d=args.dataset)).read_text())
    for variant in VARIANTS:
        for d in DEPTHS:
            rep = score_run(qrels, read_run(runs_dir / f"{variant}-d{d}.jsonl"))
            if abs(rep["mean_recall_100"] - depth0["mean_recall_100"]) > 1e-9:
                failures.append(f"{variant}-d{d}: Recall@100 {rep['mean_recall_100']} differs from depth 0 {depth0['mean_recall_100']}")
    for f in failures:
        print(f"MISMATCH {f}")
    print(f"check {args.dataset}: {'PASS' if not failures else 'FAIL'} ({4 if args.e2e_run and args.explain else 2} exactness checks, {len(VARIANTS) * len(DEPTHS)} Recall@100 checks)")
    return 0 if not failures else 1


def cmd_table(args) -> int:
    runs_dir = Path(args.runs_dir or RUNS_DIR)
    rows, depth0 = load_table_rows(runs_dir)
    (runs_dir / "table.json").write_text(json.dumps({"depth0": depth0, "rows": rows}, indent=1) + "\n")
    md = table_markdown(rows, depth0)
    (runs_dir / "table.md").write_text(md)
    print(md)
    return 0


def cmd_decide(args) -> int:
    runs_dir = Path(args.runs_dir or RUNS_DIR)
    rows, depth0 = load_table_rows(runs_dir)
    d = decide(rows, {ds: depth0[ds]["ndcg_10"] for ds in DATASETS})
    (runs_dir / "decision.json").write_text(json.dumps(d, indent=1) + "\n")
    print("rule:", json.dumps(d["rule"]))
    print("qualifying:", [f"{r['variant']}-d{r['depth']} ({r['mean']:.4f})" for r in d["qualifying"]] or "none")
    print("decision:", d["statement"])
    return 0


def cmd_all(args) -> int:
    for step in (cmd_derive, cmd_score, cmd_check):
        if (rc := step(args)) != 0:
            return rc
    return 0


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    needs = {
        "derive": ("dataset", "explain"), "score": ("dataset",), "check": ("dataset", "baseline_run", "baseline_report"),
        "table": (), "decide": (), "all": ("dataset", "explain", "baseline_run", "baseline_report"),
    }
    for name, required in needs.items():
        sp = sub.add_parser(name)
        sp.add_argument("--dataset", choices=DATASETS, required="dataset" in required)
        sp.add_argument("--explain", type=Path, required="explain" in required)
        sp.add_argument("--baseline-run", type=Path, required="baseline_run" in required)
        sp.add_argument("--baseline-report", type=Path, required="baseline_report" in required)
        sp.add_argument("--e2e-run", type=Path)
        sp.add_argument("--e2e-depth", type=int, default=5)
        sp.add_argument("--runs-dir", type=Path)
        sp.add_argument("--out-dir", type=Path)
    args = p.parse_args(argv)
    if args.cmd == "all":
        # derive writes to out-dir (the study dir); score reads runs from there and writes cells to RUNS_DIR
        study_dir = args.out_dir or STUDY_DIR / args.dataset
        args.out_dir = study_dir
        rc = cmd_derive(args)
        if rc:
            return rc
        args.runs_dir, args.out_dir = study_dir, RUNS_DIR
        return cmd_score(args) or cmd_check(args)
    return {"derive": cmd_derive, "score": cmd_score, "check": cmd_check, "table": cmd_table, "decide": cmd_decide}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
