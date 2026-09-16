#!/usr/bin/env python3
"""Feature 016 — the sparse stage re-measured against the current pipeline
(specs/016-sparse-remeasure).

012 said GO to an inference-free sparse stage on +1.6 mean nDCG@10 over a 0.468 pipeline; the
pipeline is now at 0.4913 (013 joined lexical field, 015 interpolated re-rank). This script
fuses the engine's own lexical-v2 and dense candidate lists (014's explain exports) with 012's
sparse dot list under the engine's RRF (k 60, ties by corpus position), re-ranks under the
engine's interpolating rule (014's derivation, `rerank_study`) with the engine's cross-encoder
scores where the exports cover the pair and the 006 torch reference where they do not, and
applies the decision rule fixed in the spec (FR-007).

Two anchors make the arithmetic executable: `lex2+dense-plain` must equal `hybrid-baseline-v2`
and `lex2+dense-rr` must equal `hybrid-rerank-v3`, per query and list for list.

Subcommands (contracts/remeasure-cli.md): fuse | rerank | score | check | table | decide | all.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "reference"))

import gen_003_fixtures as ref003  # noqa: E402
import rerank_study as rs  # noqa: E402

RRF_K = 60
DEPTH = 100
RERANK_DEPTH = 20
ALPHA = 0.5
DATASETS = ["scifact", "nfcorpus", "fiqa"]
SPARSE_KEY = "dot@opensearch-neural-sparse-encoding-doc-v3-distill@babf71f3"
STUDY_DIR = REPO / "target" / "xt-sparse-remeasure"
RUNS_DIR = REPO / "specs" / "016-sparse-remeasure" / "runs"
EXPLAIN = REPO / "target" / "xt-rerank-study" / "{d}" / "explain-d50.jsonl"
DOT_RUN = REPO / "target" / "xt-sparse-runs" / "{d}" / (SPARSE_KEY + ".jsonl")
V2_BASELINE = REPO / "specs" / "013-lexical-quality" / "baselines" / "hybrid-baseline-v2.{d}.json"
V3_BASELINE = REPO / "specs" / "015-rerank-interpolation" / "baselines" / "hybrid-rerank-v3.{d}.json"
V3_RUN = REPO / "target" / "xt-rr3-run.{d}.jsonl"
MODEL_DIR = REPO / "reference" / "models" / "ms-marco-MiniLM-L-6-v2"
BEIR = REPO / "reference" / "datasets" / "beir"
V3_MEAN = 0.491307  # hybrid-rerank-v3, three-set mean (015 report)
MEAN_FLOOR = 0.4963  # spec FR-007 as declared: "0.4913 + 0.005 = 0.4963" — the rule's own rounding, not re-derived here
MAX_DROP = 0.005
DECISION_ROW = "lex2+dense+dot-rr"
SPIKE_THREE_WAY = {"scifact": 0.7139, "nfcorpus": 0.3508, "fiqa": 0.3881}  # 012 rrf-lex+dense+dot (v1 lexical)

VARIANTS = {
    "lex2+dense": ("lexical", "dense"),
    "lex2+dense+dot": ("lexical", "dense", "dot"),
    "dense+dot": ("dense", "dot"),
    "lex2+dot": ("lexical", "dot"),
}


# ── inputs ───────────────────────────────────────────────────────────────────────────────────
def load_explain(dataset: str) -> list[dict]:
    path = Path(str(EXPLAIN).format(d=dataset))
    if not path.exists():
        sys.exit(f"missing {path} — the 014 depth-50 explain export")
    lines = rs.read_explain(path)
    for line in lines:
        if len(line["rerank"]) < min(50, len(line["fused"])):
            sys.exit(f"{path}: query {line['query_id']} has {len(line['rerank'])} scored pairs; 016 needs the depth-50 export")
    return lines


def load_dot_run(dataset: str) -> dict[str, list[str]]:
    path = Path(str(DOT_RUN).format(d=dataset))
    if not path.exists():
        sys.exit(f"missing {path} — 012's sparse dot run")
    return rs.read_run(path)


def load_queries(dataset: str) -> dict[str, str]:
    out = {}
    with (BEIR / dataset / "queries.jsonl").open("rb") as f:
        for raw in f:
            q = json.loads(raw)
            out[q["_id"]] = q["text"]
    return out


def load_passages(dataset: str) -> dict[str, str]:
    """The pipeline's passage: `title + " " + text`, an empty side omitted (013)."""
    out = {}
    with (BEIR / dataset / "corpus.jsonl").open("rb") as f:
        for raw in f:
            d = json.loads(raw)
            title, text = d.get("title") or "", d.get("text") or ""
            out[d["_id"]] = text if not title else (title if not text else f"{title} {text}")
    return out


# ── the engine's fusion ──────────────────────────────────────────────────────────────────────
def fuse(lists: list[list[str]], positions: dict[str, int], k: int = RRF_K, depth: int = DEPTH) -> list[tuple[str, float]]:
    """`xtriever_pipeline::fusion::fused_terms`: Σ 1/(k + rank) over the lists an id appears in
    (1-based rank, each list cut at `depth`), sorted by (−sum, corpus position), cut at `depth`."""
    scores: dict[str, float] = {}
    for ranked in lists:
        for rank0, did in enumerate(ranked[:depth]):
            scores[did] = scores.get(did, 0.0) + 1.0 / (k + rank0 + 1)
    ordered = sorted(scores.items(), key=lambda kv: (-kv[1], positions[kv[0]]))
    return ordered[:depth]


def candidate_lists(line: dict, dot: dict[str, list[str]], names: tuple[str, ...]) -> list[list[str]]:
    by_name = {
        "lexical": [i for _, i in sorted(line["lexical"])],
        "dense": [i for _, i in sorted(line["dense"])],
        "dot": dot.get(line["query_id"], []),
    }
    return [by_name[n] for n in names]


# ── cross-encoder scores: the engine's where covered, the reference otherwise ───────────────
class ScoreCache:
    """`{query_id, doc_id, score}` lines; every reference score is written once and reused."""

    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        self.scores: dict[tuple[str, str], float] = {}
        if self.path.exists():
            for raw in self.path.read_text().splitlines():
                if raw.strip():
                    r = json.loads(raw)
                    self.scores[(r["query_id"], r["doc_id"])] = r["score"]

    def get(self, qid: str, did: str) -> float | None:
        return self.scores.get((qid, did))

    def put(self, qid: str, did: str, score: float) -> None:
        self.scores[(qid, did)] = score
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a") as f:
            f.write(json.dumps({"query_id": qid, "doc_id": did, "score": score}) + "\n")


class StubCounting:
    """Wrap a scorer to count calls (tests)."""

    def __init__(self, inner) -> None:
        self.inner, self.calls = inner, 0

    def score(self, query: str, passage: str) -> float:
        self.calls += 1
        return self.inner.score(query, passage)


class ReferenceScorer:
    """The 006 reference cross-encoder, loaded on the first uncovered pair."""

    def __init__(self, model_dir: Path) -> None:
        self.model_dir, self._ref, self.calls = Path(model_dir), None, 0

    def score(self, query: str, passage: str) -> float:
        if self._ref is None:
            import gen_006_fixtures as ref006

            # Every pinned file's size and SHA-256 (the 006 manifest), not just the weights' presence:
            # the reference scores must come from the same bytes the engine runs (spec FR-004).
            ref006.verify_model_dir(self.model_dir, ref006.load_pins())
            self._ref = ref006.Reference(self.model_dir)
        self.calls += 1
        return float(self._ref.score(query, passage)["score"])


def head_scores(fused, line, cache: ScoreCache, scorer, *, query_text: str, passages: dict[str, str], depth: int = RERANK_DEPTH):
    """`[(id, score, source)]` for the first `depth` fused ids: `engine` from the explain's
    `rerank` map, else `reference` (cached)."""
    engine = {i: s for _, i, s in line["rerank"]}
    qid = line["query_id"]
    out = []
    for did, _ in fused[:depth]:
        if did in engine:
            out.append((did, engine[did], "engine"))
            continue
        s = cache.get(qid, did)
        if s is None:
            s = scorer.score(query_text, passages[did])
            cache.put(qid, did, s)
        out.append((did, s, "reference"))
    return out


def reference_share(head) -> float:
    return sum(src == "reference" for _, _, src in head) / len(head) if head else 0.0


def rerank_fused(fused, head, positions: dict[str, int], depth: int = RERANK_DEPTH) -> list[str]:
    """014's interpolating rule (`rerank_study.order_lin`, α 0.5) over the head; the tail in
    fused order."""
    score_of = {i: s for i, s, _ in head}
    entries = [
        {"id": i, "pos": positions[i], "r_f": r + 1, "s_f": s_f, "s_c": score_of[i]}
        for r, (i, s_f) in enumerate(fused[:depth])
        if i in score_of
    ]
    head_ids = {e["id"] for e in entries}
    tail = [i for i, _ in fused if i not in head_ids]
    return rs.order_lin(entries, tail, DEPTH, ALPHA)


# ── scoring, checking, tabulating, deciding ──────────────────────────────────────────────────
def study_dir(dataset: str, override: Path | None) -> Path:
    return Path(override) if override else STUDY_DIR / dataset


def cell_path(runs_dir: Path, variant: str, reranked: bool, dataset: str) -> Path:
    return runs_dir / f"{variant}-{'rr' if reranked else 'plain'}.{dataset}.json"


def decide(row: dict, v3: dict[str, float], *, mean_floor: float = MEAN_FLOOR, max_drop: float = MAX_DROP) -> dict:
    """FR-007: the `lex2+dense+dot-rr` row qualifies iff its mean ≥ v3 mean + 0.005 and no
    dataset is more than 0.005 below hybrid-rerank-v3."""
    drops = {d: row["ndcg"][d] - v3[d] for d in v3}
    qualifies = row["mean"] >= mean_floor - 1e-9 and all(v >= -max_drop - 1e-9 for v in drops.values())
    statement = (
        f"specify the sparse stage: {DECISION_ROW} mean {row['mean']:.4f} ≥ {mean_floor} with no dataset below hybrid-rerank-v3 by more than {max_drop}"
        if qualifies
        else f"012's GO withdrawn: {DECISION_ROW} mean {row['mean']:.4f} against the floor {mean_floor}; per-dataset deltas vs hybrid-rerank-v3 "
        + ", ".join(f"{d} {v:+.4f}" for d, v in drops.items())
    )
    return {
        "rule": {"row": DECISION_ROW, "mean_floor": mean_floor, "max_drop": max_drop, "anchor": "hybrid-rerank-v3", "anchor_mean": V3_MEAN},
        "row": DECISION_ROW,
        "cells": {"ndcg": row["ndcg"], "mean": row["mean"], "delta_vs_v3": drops},
        "qualifies": qualifies,
        "statement": statement,
        "reopen": [
            "a corpus with heavy vocabulary mismatch (FiQA-shaped: no titles, colloquial queries) where a dense-free configuration is wanted",
            "an encoder that is cheaper to run at index time or runnable inside the Rust pipeline (012 F-004: corpus encoding must happen outside Rust today)",
            "a measured need at the head that the cross-encoder does not cover — e.g. a re-rank depth the phone cannot afford",
            "a change to the fused signal (new dense model, new lexical layout) that re-opens the candidate-set question",
        ],
    }


def cmd_fuse(args) -> int:
    lines = load_explain(args.dataset)
    dot = load_dot_run(args.dataset)
    positions = rs.corpus_positions(args.dataset)
    out = study_dir(args.dataset, args.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    for variant, names in VARIANTS.items():
        run, fused_all = {}, {}
        for line in lines:
            fused = fuse(candidate_lists(line, dot, names), positions)
            run[line["query_id"]] = [i for i, _ in fused]
            fused_all[line["query_id"]] = fused
        rs.write_run(out / f"{variant}-plain.jsonl", run)
        (out / f"{variant}-plain.fused.json").write_text(json.dumps(fused_all))
    print(f"fused {len(VARIANTS)} variants × {len(lines)} queries into {out}")
    return 0


def cmd_rerank(args) -> int:
    lines = {l["query_id"]: l for l in load_explain(args.dataset)}
    positions = rs.corpus_positions(args.dataset)
    queries = load_queries(args.dataset)
    passages = load_passages(args.dataset)
    out = study_dir(args.dataset, args.out_dir)
    cache = ScoreCache(out / "reference-scores.jsonl")
    scorer = ReferenceScorer(args.model_dir or MODEL_DIR)
    stats = {}
    for variant in VARIANTS:
        fused_all = json.loads((out / f"{variant}-plain.fused.json").read_text())
        run, pairs, from_ref = {}, 0, 0
        for qid, fused in fused_all.items():
            fused = [(i, s) for i, s in fused]
            head = head_scores(fused, lines[qid], cache, scorer, query_text=queries[qid], passages=passages)
            pairs += len(head)
            from_ref += sum(src == "reference" for _, _, src in head)
            run[qid] = rerank_fused(fused, head, positions)
        rs.write_run(out / f"{variant}-rr.jsonl", run)
        stats[variant] = {"head_pairs": pairs, "engine": pairs - from_ref, "reference": from_ref, "reference_share": from_ref / pairs if pairs else 0.0}
        print(f"{args.dataset} {variant}-rr: head pairs {pairs}, engine {pairs - from_ref}, reference {from_ref} ({100 * stats[variant]['reference_share']:.1f}%)")
    # Agreement of the reference with the engine on covered pairs (research D4).
    n = args.agreement_sample
    sample, diffs = [], []
    for qid in sorted(lines):
        for _, did, s in lines[qid]["rerank"]:
            sample.append((qid, did, s))
            if len(sample) >= n:
                break
        if len(sample) >= n:
            break
    for qid, did, engine_score in sample:
        diffs.append(abs(scorer.score(queries[qid], passages[did]) - engine_score))
    agreement = {"sampled": len(diffs), "max_abs_diff": max(diffs) if diffs else None, "mean_abs_diff": sum(diffs) / len(diffs) if diffs else None}
    print(f"{args.dataset} reference vs engine on {agreement['sampled']} covered pairs: max |Δ| {agreement['max_abs_diff']:.6f}, mean |Δ| {agreement['mean_abs_diff']:.6f}")
    (out / "rerank-stats.json").write_text(json.dumps({"variants": stats, "agreement": agreement, "reference_calls": scorer.calls}, indent=1) + "\n")
    return 0


def cmd_score(args) -> int:
    ref003.probe()
    qrels = rs.qrels_for(args.dataset)
    src = study_dir(args.dataset, args.runs_dir)
    out = Path(args.out_dir or RUNS_DIR)
    out.mkdir(parents=True, exist_ok=True)
    stats = json.loads((src / "rerank-stats.json").read_text()) if (src / "rerank-stats.json").exists() else None
    for variant in VARIANTS:
        for reranked in (False, True):
            path = src / f"{variant}-{'rr' if reranked else 'plain'}.jsonl"
            if not path.exists():
                continue
            rep = rs.score_run(qrels, rs.read_run(path))
            cell = {
                "variant": variant, "reranked": reranked, "dataset": args.dataset, "source": "offline",
                "ndcg_10": rep["mean_ndcg_10"], "recall_100": rep["mean_recall_100"],
                "beir_rounded": rep["beir_rounded"], "scored_queries": rep["scored_queries"],
            }
            if reranked:
                if not stats:
                    sys.exit(f"{src / 'rerank-stats.json'} is missing: re-ranked runs cannot be scored without their reference-pair counts (run `rerank` first)")
                v = stats["variants"][variant]
                cell.update({"reference_scored_pairs": v["reference"], "head_pairs": v["head_pairs"], "reference_share": v["reference_share"], "agreement": stats["agreement"]})
            cell["per_query"] = rep["per_query"]
            cell_path(out, variant, reranked, args.dataset).write_text(json.dumps(cell, indent=1, sort_keys=True) + "\n")
            print(f"{args.dataset} {variant}-{'rr' if reranked else 'plain'}: nDCG@10={rep['mean_ndcg_10']:.6f} Recall@100={rep['mean_recall_100']:.6f}")
    return 0


def cmd_check(args) -> int:
    src = study_dir(args.dataset, args.runs_dir)
    runs_dir = Path(args.out_dir or RUNS_DIR)
    qrels = rs.qrels_for(args.dataset)
    failures = []
    # FR-002: the anchor fusion is the engine's fused order and the 013 baseline.
    lines = load_explain(args.dataset)
    plain = rs.read_run(src / "lex2+dense-plain.jsonl")
    if err := rs.compare_runs("lex2+dense-plain vs explain fused", plain, {l["query_id"]: l["fused"] for l in lines}):
        failures.append(err)
    v2 = json.loads(Path(str(V2_BASELINE).format(d=args.dataset)).read_text())
    if err := rs.compare_metrics("lex2+dense-plain vs hybrid-baseline-v2", rs.score_run(qrels, plain), v2):
        failures.append(err)
    # FR-003: the anchor re-ranking is hybrid-rerank-v3, with no reference pair.
    rr_path = src / "lex2+dense-rr.jsonl"
    if rr_path.exists():
        rr = rs.read_run(rr_path)
        if err := rs.compare_runs("lex2+dense-rr vs xt-rr3-run", rr, rs.read_run(Path(str(V3_RUN).format(d=args.dataset)))):
            failures.append(err)
        v3 = json.loads(Path(str(V3_BASELINE).format(d=args.dataset)).read_text())
        if err := rs.compare_metrics("lex2+dense-rr vs hybrid-rerank-v3", rs.score_run(qrels, rr), v3):
            failures.append(err)
        stats = json.loads((src / "rerank-stats.json").read_text())
        if stats["variants"]["lex2+dense"]["reference"] != 0:
            failures.append(f"lex2+dense-rr used {stats['variants']['lex2+dense']['reference']} reference pairs; every v2 head pair is covered")
        # Recall@100: plain = rr for every variant.
        for variant in VARIANTS:
            a = rs.score_run(qrels, rs.read_run(src / f"{variant}-plain.jsonl"))["mean_recall_100"]
            b = rs.score_run(qrels, rs.read_run(src / f"{variant}-rr.jsonl"))["mean_recall_100"]
            if abs(a - b) > 1e-9:
                failures.append(f"{variant}: Recall@100 plain {a} vs rr {b}")
    for f in failures:
        print(f"MISMATCH {f}")
    print(f"check {args.dataset}: {'PASS' if not failures else 'FAIL'} ({'plain + rr' if rr_path.exists() else 'plain only'})")
    _ = runs_dir
    return 0 if not failures else 1


def load_rows(runs_dir: Path):
    v2 = {d: json.loads(Path(str(V2_BASELINE).format(d=d)).read_text())["mean_ndcg_10"] for d in DATASETS}
    v3 = {d: json.loads(Path(str(V3_BASELINE).format(d=d)).read_text())["mean_ndcg_10"] for d in DATASETS}
    rows = []
    for variant in VARIANTS:
        for reranked in (False, True):
            cells = {}
            for d in DATASETS:
                p = cell_path(runs_dir, variant, reranked, d)
                if p.exists():
                    cells[d] = json.loads(p.read_text())
            if len(cells) != len(DATASETS):
                continue
            rows.append({
                "variant": variant, "reranked": reranked,
                "ndcg": {d: cells[d]["ndcg_10"] for d in DATASETS},
                "recall": {d: cells[d]["recall_100"] for d in DATASETS},
                "mean": sum(cells[d]["ndcg_10"] for d in DATASETS) / len(DATASETS),
                "reference_share": {d: cells[d].get("reference_share") for d in DATASETS} if reranked else None,
            })
    return rows, v2, v3


def table_markdown(rows, v2, v3) -> str:
    def anchor_row(name, vals):
        return f"| {name} | " + " | ".join(f"{vals[d]:.4f}" for d in DATASETS) + f" | {sum(vals.values()) / 3:.4f} | — |"

    out = ["| row | " + " | ".join(DATASETS) + " | mean | reference share |", "|---|" + "---|" * (len(DATASETS) + 2)]
    out.append(anchor_row("hybrid-baseline-v2 (anchor, plain)", v2))
    out.append(anchor_row("012 rrf-lex+dense+dot (v1 lexical, plain)", SPIKE_THREE_WAY))
    out.append(anchor_row("hybrid-rerank-v3 (anchor, rr)", v3))
    for r in rows:
        anchor = v3 if r["reranked"] else v2
        cells = " | ".join(f"{r['ndcg'][d]:.4f} ({r['ndcg'][d] - anchor[d]:+.4f})" for d in DATASETS)
        share = ", ".join(f"{100 * r['reference_share'][d]:.0f}%" for d in DATASETS) if r["reranked"] else "—"
        out.append(f"| {r['variant']}-{'rr' if r['reranked'] else 'plain'} | {cells} | {r['mean']:.4f} ({r['mean'] - sum(anchor.values()) / 3:+.4f}) | {share} |")
    return "\n".join(out) + "\n"


def cmd_table(args) -> int:
    runs_dir = Path(args.runs_dir or RUNS_DIR)
    rows, v2, v3 = load_rows(runs_dir)
    (runs_dir / "table.json").write_text(json.dumps({"anchors": {"hybrid-baseline-v2": v2, "hybrid-rerank-v3": v3, "012-three-way": SPIKE_THREE_WAY}, "rows": rows}, indent=1) + "\n")
    md = table_markdown(rows, v2, v3)
    (runs_dir / "table.md").write_text(md)
    print(md)
    return 0


def cmd_decide(args) -> int:
    runs_dir = Path(args.runs_dir or RUNS_DIR)
    rows, _, v3 = load_rows(runs_dir)
    row = next((r for r in rows if r["variant"] == "lex2+dense+dot" and r["reranked"]), None)
    if row is None:
        print("no lex2+dense+dot-rr cells on all three datasets yet")
        return 1
    d = decide(row, v3)
    if args.owner_decision:
        # A recorded decision the rule does not make (what happens *besides* the default), kept
        # in a hand-written file so `decide` stays reproducible and never loses it.
        d["owner_decision"] = json.loads(Path(args.owner_decision).read_text())
    (runs_dir / "decision.json").write_text(json.dumps(d, indent=1) + "\n")
    print("rule:", json.dumps(d["rule"]))
    print("cells:", json.dumps(d["cells"]))
    print("decision:", d["statement"])
    return 0


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("fuse", "rerank", "score", "check", "table", "decide", "all"):
        sp = sub.add_parser(name)
        sp.add_argument("--dataset", choices=DATASETS, required=name not in ("table", "decide"))
        sp.add_argument("--out-dir", type=Path)
        sp.add_argument("--runs-dir", type=Path)
        sp.add_argument("--model-dir", type=Path)
        sp.add_argument("--agreement-sample", type=int, default=200)
        sp.add_argument("--owner-decision", type=Path, help="decide: a JSON file with the owner's decision to embed (see specs/016-sparse-remeasure/owner-decision.json)")
    args = p.parse_args(argv)
    if args.cmd == "all":
        for step in (cmd_fuse, cmd_rerank):
            if (rc := step(args)) != 0:
                return rc
        # `--out-dir` named the study directory for fuse/rerank; score and check read it back
        # as their runs directory and write the cells to the committed runs/ directory.
        args.runs_dir, args.out_dir = study_dir(args.dataset, args.out_dir), None
        return cmd_score(args) or cmd_check(args)
    return {"fuse": cmd_fuse, "rerank": cmd_rerank, "score": cmd_score, "check": cmd_check, "table": cmd_table, "decide": cmd_decide}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
