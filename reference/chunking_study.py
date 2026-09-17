#!/usr/bin/env python3
"""The chunking study (Feature 022, specs/022-chunking-study): does splitting long documents
into passages change retrieval quality, and which splitter is best?

The engine indexes each BEIR document as one passage whose embedding is cut at the
embedder's 256-position window — 71 % of SciFact and 79 % of NFCorpus documents are longer.
This script indexes each corpus four ways through the Python package, retrieves passages,
folds them back to documents by their best passage (MaxP) and scores exactly as the
baselines were scored (pytrec_eval, `gen_003_fixtures.reference`).

Variants:
  whole           one passage per document (today's behaviour; the anchor)
  contract        the Feature 008 contract chunker, budget 256 − title positions
  chonky          the pinned chonky paragraph splitter, unbounded
  chonky-bounded  chonky, then fragments (< 16 positions) merged into their predecessor and
                  any chunk over the window re-chunked by the contract chunker

Cells are `<variant>-d<rerank depth>@<k>.<dataset>`: `@100` (k = 100, candidate depth 100)
is the harness's configuration and exists only for `whole` — it must reproduce the committed
baselines (`hybrid-baseline-v2` fused, `hybrid-rerank-v3` re-ranked) to 1e-6 per query before
any variant runs; `@300` is the study's configuration for every variant.

Decision rule (spec US4): a chunker is recommended if its re-ranked mean nDCG@10 over the
datasets it ran on is ≥ whole's + MEAN_GAIN, no dataset is more than MAX_DROP below whole,
and mean Recall@100 is not more than RECALL_DROP below; the best chunker by the same rule,
ties to the one without a neural splitter (`contract`).

    PY=reference/.venv-022/bin/python
    $PY reference/chunking_study.py build  --variant whole --dataset scifact
    $PY reference/chunking_study.py search --variant whole --dataset scifact --depth 20 --k 100
    $PY reference/chunking_study.py score  --dataset scifact
    $PY reference/chunking_study.py check  --dataset scifact          # the anchor
    $PY reference/chunking_study.py all    --dataset scifact          # everything for one dataset
    $PY reference/chunking_study.py table; $PY reference/chunking_study.py decide

Cost on the reference laptop: ~100 ms per passage to embed, ~80 ms per re-ranked pair —
SciFact ~1 h 40, NFCorpus ~1 h 30, FiQA ~2 h 10 for `whole` and ~2 h 35 per chunked variant.
Everything is resumable: a finished index or run file is never recomputed.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "reference"))

import gen_003_fixtures as ref003  # noqa: E402  (the scorer)
import gen_008_fixtures as ref008  # noqa: E402  (the contract chunker's reference implementation)
import rerank_study as rs  # noqa: E402  (run files, metric comparison)

# ── constants (spec US4 / FR-004; asserted by reference/tests_022) ─────────────────────────
WINDOW = 256
MIN_POSITIONS = 16
MEAN_GAIN = 0.005
MAX_DROP = 0.005
RECALL_DROP = 0.005
K_STUDY = 300
K_ANCHOR = 100
DEPTHS = (0, 20)
DOCS_PER_QUERY = 100
BATCH = 4_096
DATASETS = ("scifact", "nfcorpus", "fiqa")
VARIANTS = ("whole", "contract", "chonky", "chonky-bounded")
CHUNKERS = ("contract", "chonky", "chonky-bounded")

BEIR = REPO / "reference/datasets/beir"
RUNS_DIR = REPO / "specs/022-chunking-study/runs"
STUDY_DIR = REPO / "target/xt-chunking-study"
EMBEDDER = Path(os.environ.get("XTRIEVER_MODEL_DIR", REPO / "reference/models/all-MiniLM-L6-v2"))
RERANKER = Path(os.environ.get("XTRIEVER_RERANK_MODEL_DIR", REPO / "reference/models/ms-marco-MiniLM-L-6-v2"))
CHONKY = Path(os.environ.get("XTRIEVER_CHONKY_MODEL_DIR", REPO / "reference/models/chonky_distilbert_base_uncased_1"))
BASELINES = {
    0: REPO / "specs/013-lexical-quality/baselines/hybrid-baseline-v2.{dataset}.json",
    20: REPO / "specs/015-rerank-interpolation/baselines/hybrid-rerank-v3.{dataset}.json",
}


class StudyError(Exception):
    """A step that must stop: the message names what and where."""


# ── the baselines' document shaping (research D3) ──────────────────────────────────────────


def join_title_text(title: str, text: str) -> str:
    """`title + " " + text`; an empty part contributes nothing and no separator."""
    if title and text:
        return f"{title} {text}"
    return title or text


def passage_contents(title: str, chunk: str) -> str:
    return join_title_text(title, chunk)


# ── the splitters (research D5) ─────────────────────────────────────────────────────────────


class Cost:
    """The embedder's tokenizer: `token_count` in positions, `cost` in content pieces."""

    def __init__(self, embedder_dir: Path):
        from tokenizers import Tokenizer

        self._tok = Tokenizer.from_file(str(Path(embedder_dir) / "tokenizer.json"))
        self._tok.no_truncation()
        self._tok.no_padding()
        self._memo: dict[str, int] = {}

    def token_count(self, s: str) -> int:
        n = self._memo.get(s)
        if n is None:
            n = len(self._tok.encode(s, add_special_tokens=True).ids)
            self._memo[s] = n
        return n

    def cost(self, unit: str) -> int:
        return self.token_count(unit) - 2


def split_contract(text: str, cost, title_positions: int) -> tuple[list[str], bool]:
    """The 008 contract chunker with budget `WINDOW − title_positions`; a title that alone
    fills the window leaves the document whole (flagged)."""
    if title_positions >= WINDOW:
        return [text], True
    pieces = [p["text"] for p in ref008.chunk(text, WINDOW - title_positions, cost)]
    return pieces, False


def chonky_passages(text: str, splitter) -> list[str]:
    """chonky's slices, stripped, empty ones dropped; the slices must partition the text."""
    pieces = list(splitter(text))
    if "".join(pieces) != text:
        raise StudyError("the splitter did not return a partition of the text")
    return [p.strip() for p in pieces if p.strip()]


def bound_and_merge(chunks: list[str], cost, positions, title_positions: int) -> tuple[list[str], int]:
    """chonky-bounded: (a) fragments under MIN_POSITIONS merged into the preceding chunk (the
    first into the following), (b) chunks over the window re-chunked by the contract chunker,
    (c) a remainder under MIN_POSITIONS merged back only when the result fits. Returns the
    passages and the number of under-MIN_POSITIONS passages kept because merging would breach
    the window."""
    budget = WINDOW - title_positions

    def merged_fragments(items: list[str]) -> list[str]:
        out: list[str] = []
        leading: str | None = None  # fragments before any full chunk wait to join the next one
        for chunk in items:
            if positions(chunk) < MIN_POSITIONS:
                if out:
                    out[-1] = out[-1] + "\n" + chunk
                else:
                    leading = chunk if leading is None else leading + "\n" + chunk
                continue
            if leading is not None:
                chunk = leading + "\n" + chunk
                leading = None
            out.append(chunk)
        if leading is not None:  # the whole document is fragments
            out.append(leading)
        return out

    merged = merged_fragments(chunks)
    bounded: list[str] = []
    for chunk in merged:
        if cost(chunk) > budget:
            bounded.extend(p["text"] for p in ref008.chunk(chunk, budget, cost))
        else:
            bounded.append(chunk)
    kept = 0
    final: list[str] = []
    for chunk in bounded:
        if positions(chunk) < MIN_POSITIONS and final and cost(final[-1] + "\n" + chunk) <= budget:
            final[-1] = final[-1] + "\n" + chunk
        else:
            if positions(chunk) < MIN_POSITIONS and len(bounded) > 1:
                kept += 1
            final.append(chunk)
    return final, kept


def passages_for(variant: str, doc: dict, cost, positions, splitter) -> tuple[list[str], dict]:
    """The document's passages under a variant, plus the counters of the build record."""
    title, text = doc.get("title", ""), doc.get("text", "")
    counters = {"over_window": 0, "under_16": 0, "title_fills_window": 0}
    title_positions = positions(title) if title else 2
    if variant == "whole":
        passages = [text]
    elif variant == "contract":
        passages, flag = split_contract(text, cost, title_positions)
        counters["title_fills_window"] += flag
    elif variant == "chonky":
        passages = chonky_passages(text, splitter)
    elif variant == "chonky-bounded":
        passages, kept = bound_and_merge(chonky_passages(text, splitter), cost, positions, title_positions)
        counters["under_16"] += kept
    else:
        raise StudyError(f"unknown variant {variant!r}")
    for p in passages:
        if positions(passage_contents(title, p)) > WINDOW:
            counters["over_window"] += 1
    return passages, counters


# ── BEIR, the index, the cells ──────────────────────────────────────────────────────────────


def load_corpus(dataset: str, limit: int | None = None) -> list[dict]:
    docs = []
    with (BEIR / dataset / "corpus.jsonl").open("rb") as fh:
        for line in fh:
            docs.append(json.loads(line))
            if limit is not None and len(docs) >= limit:
                break
    return docs


def qrels_for(dataset: str) -> dict:
    return ref003.load_qrels_tsv(BEIR / dataset / "qrels" / "test.tsv")


def load_queries(dataset: str) -> dict[str, str]:
    """Only the queries the test qrels judge (the ones the baselines scored)."""
    judged = set(qrels_for(dataset))
    out = {}
    with (BEIR / dataset / "queries.jsonl").open("rb") as fh:
        for line in fh:
            q = json.loads(line)
            if q["_id"] in judged:
                out[q["_id"]] = q["text"]
    return out


def index_dir(variant: str, dataset: str, limit: int | None = None) -> Path:
    suffix = f".limit{limit}" if limit else ""
    return STUDY_DIR / f"{variant}.{dataset}{suffix}"


def chonky_cache_path(dataset: str, limit: int | None = None) -> Path:
    return STUDY_DIR / f"chonky.{dataset}{'.limit' + str(limit) if limit else ''}.jsonl"


class ChonkySplits:
    """chonky's slices per document, computed once per dataset and cached on disk."""

    def __init__(self, dataset: str, limit: int | None):
        self.path = chonky_cache_path(dataset, limit)
        self.cache: dict[str, list[str]] = {}
        self._splitter = None
        if self.path.exists():
            for line in self.path.read_text(encoding="utf-8").splitlines():
                rec = json.loads(line)
                self.cache[rec["_id"]] = rec["pieces"]

    def splitter_for(self, doc_id: str, text: str):
        """A callable yielding the cached pieces, splitting with chonky on a miss."""
        if doc_id not in self.cache:
            if self._splitter is None:
                from chonky import ParagraphSplitter

                self._splitter = ParagraphSplitter(model_id=str(CHONKY), device="cpu")
            self.cache[doc_id] = list(self._splitter(text))
            self.path.parent.mkdir(parents=True, exist_ok=True)
            with self.path.open("a", encoding="utf-8") as fh:
                fh.write(json.dumps({"_id": doc_id, "pieces": self.cache[doc_id]}, ensure_ascii=False) + "\n")
        pieces = self.cache[doc_id]
        return lambda _text: iter(pieces)


def build(variant: str, dataset: str, limit: int | None = None) -> dict:
    """Index a corpus under a variant; skipped when its build record exists."""
    import xtriever

    out = index_dir(variant, dataset, limit)
    record_path = out / "build.json"
    if record_path.exists():
        return json.loads(record_path.read_text(encoding="utf-8"))
    for p in (EMBEDDER / "model.safetensors", RERANKER / "model.safetensors", BEIR / dataset / "corpus.jsonl"):
        if not p.exists():
            raise StudyError(f"missing input {p}")
    if variant.startswith("chonky") and not (CHONKY / "model.safetensors").exists():
        raise StudyError(f"missing the chonky model at {CHONKY}: scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json")
    docs = load_corpus(dataset, limit)
    cost = Cost(EMBEDDER)
    splits = ChonkySplits(dataset, limit) if variant.startswith("chonky") else None
    config = xtriever.IndexConfig(
        fields=[xtriever.FieldDef(name="contents", kind=xtriever.FieldKind.TEXT(analyzer="standard_en"))],
        dense_fields=["contents"],
    )
    staging = out.with_name(out.name + ".partial")
    if staging.exists():
        import shutil

        shutil.rmtree(staging)
    (staging / "index").parent.mkdir(parents=True, exist_ok=True)
    handle = xtriever.IndexHandle.create(str(staging / "index"), config, str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP)
    counters = {"over_window": 0, "under_16": 0, "title_fills_window": 0}
    per_doc: list[int] = []
    batch: list = []
    split_s = embed_s = 0.0
    passages_total = 0
    t_start = time.perf_counter()

    def flush():
        nonlocal embed_s, batch
        if batch:
            t = time.perf_counter()
            handle.add(batch)
            embed_s += time.perf_counter() - t
            print(f"  {variant}.{dataset}: {passages_total} passages, {round(time.perf_counter() - t_start)} s", file=sys.stderr)
            batch = []

    for doc in docs:
        t = time.perf_counter()
        splitter = splits.splitter_for(doc["_id"], doc.get("text", "")) if splits else None
        try:
            passages, c = passages_for(variant, doc, cost.cost, cost.token_count, splitter)
        except StudyError as e:
            raise StudyError(f"document {doc['_id']}: {e}") from None
        split_s += time.perf_counter() - t
        for k in counters:
            counters[k] += c[k]
        per_doc.append(len(passages))
        title = doc.get("title", "")
        for i, p in enumerate(passages):
            if variant == "whole":
                batch.append(xtriever.Document(external_id=doc["_id"], fields={"contents": xtriever.FieldValue.TEXT(passage_contents(title, p))}))
            else:
                batch.append(
                    xtriever.Document(
                        external_id=f"{doc['_id']}#{i}",
                        fields={"contents": xtriever.FieldValue.TEXT(passage_contents(title, p))},
                        chunk=xtriever.ChunkInfo(parent=doc["_id"], ordinal=i, byte_start=None, byte_end=None),
                    )
                )
        passages_total += len(passages)
        if len(batch) >= BATCH:
            flush()
    flush()
    handle.commit()
    handle.merge()
    del handle
    record = {
        "variant": variant,
        "dataset": dataset,
        "limit": limit,
        "documents": len(docs),
        "passages": passages_total,
        "passages_per_doc_median": statistics.median(per_doc) if per_doc else 0,
        **counters,
        "split_s": round(split_s, 1),
        "embed_s": round(embed_s, 1),
        "total_s": round(time.perf_counter() - t_start, 1),
    }
    (staging / "build.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    os.rename(staging, out)
    return record


def maxp(hits) -> tuple[list[str], bool]:
    """Documents by their best passage — the first occurrence in the engine's ordered hits —
    truncated to DOCS_PER_QUERY; `short` when fewer distinct documents were retrieved."""
    seen: list[str] = []
    have: set[str] = set()
    for h in hits:
        doc = h.chunk.parent if getattr(h, "chunk", None) is not None else h.external_id
        if doc not in have:
            have.add(doc)
            seen.append(doc)
            if len(seen) == DOCS_PER_QUERY:
                break
    return seen, len(seen) < DOCS_PER_QUERY


def cell_name(variant: str, depth: int, k: int, dataset: str) -> str:
    return f"{variant}-d{depth}@{k}.{dataset}"


def _runs_dir(limit: int | None) -> Path:
    """Committed cells live under RUNS_DIR; a `--limit` smoke never names one (contract)."""
    return RUNS_DIR if limit is None else STUDY_DIR / f"runs.limit{limit}"


def run_path(variant, depth, k, dataset, limit=None) -> Path:
    return _runs_dir(limit) / f"{cell_name(variant, depth, k, dataset)}.jsonl"


def score_path(variant, depth, k, dataset, limit=None) -> Path:
    return _runs_dir(limit) / f"{cell_name(variant, depth, k, dataset)}.json"


def meta_path(variant, depth, k, dataset, limit=None) -> Path:
    return _runs_dir(limit) / f"{cell_name(variant, depth, k, dataset)}.meta.json"


read_run = rs.read_run
write_run = rs.write_run


def write_meta(variant, depth, k, dataset, meta: dict, limit=None) -> None:
    p = meta_path(variant, depth, k, dataset, limit)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")


def search(variant: str, dataset: str, depth: int, k: int, limit: int | None = None) -> Path:
    """Every judged query through the index; MaxP; the run file (skipped when present)."""
    import xtriever

    path = run_path(variant, depth, k, dataset, limit)
    if path.exists():
        return path
    idx = index_dir(variant, dataset, limit) / "index"
    if not (idx / "xtriever-pipeline.json").exists():
        raise StudyError(f"no index for {variant}.{dataset}: run `build` first")
    handle = xtriever.IndexHandle.open(str(idx), str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP)
    queries = load_queries(dataset)
    run: dict[str, list[str]] = {}
    short = 0
    t0 = time.perf_counter()
    for n, (qid, text) in enumerate(sorted(queries.items()), 1):
        r = handle.search(text, xtriever.SearchOptions(k=k, depth=k, rerank_depth=depth))
        run[qid], is_short = maxp(r.hits)
        short += is_short
        if n % 50 == 0:
            print(f"  {cell_name(variant, depth, k, dataset)}: {n}/{len(queries)} queries, {round(time.perf_counter() - t0)} s", file=sys.stderr)
    write_run(path, run)
    write_meta(variant, depth, k, dataset, {"queries": len(queries), "short_queries": short, "search_s": round(time.perf_counter() - t0, 1)}, limit)
    return path


def score_cell(variant: str, depth: int, k: int, dataset: str, qrels: dict) -> dict:
    report = ref003.reference(qrels, read_run(run_path(variant, depth, k, dataset)))
    report["cell"] = cell_name(variant, depth, k, dataset)
    mp = meta_path(variant, depth, k, dataset)
    meta = json.loads(mp.read_text(encoding="utf-8")) if mp.exists() else {}
    report["queries"] = meta.get("queries")
    report["short_queries"] = meta.get("short_queries")
    report["search_s"] = meta.get("search_s")
    score_path(variant, depth, k, dataset).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def score_dataset(dataset: str, variants=VARIANTS) -> list[dict]:
    qrels = qrels_for(dataset)
    out = []
    for variant in variants:
        for depth in DEPTHS:
            for k in ((K_ANCHOR, K_STUDY) if variant == "whole" else (K_STUDY,)):
                if run_path(variant, depth, k, dataset).exists():
                    out.append(score_cell(variant, depth, k, dataset, qrels))
    return out


def check_anchor(name: str, got: dict, want: dict) -> str | None:
    """`whole@100` against the committed baseline: every query and the means within 1e-6."""
    return rs.compare_metrics(name, got, want)


def check(dataset: str) -> list[str]:
    """The anchor: whole-d0@100 vs hybrid-baseline-v2, whole-d20@100 vs hybrid-rerank-v3."""
    problems = []
    for depth, template in BASELINES.items():
        sp = score_path("whole", depth, K_ANCHOR, dataset)
        if not sp.exists():
            problems.append(f"{sp.name}: not scored yet")
            continue
        got = json.loads(sp.read_text(encoding="utf-8"))
        want = json.loads(Path(str(template).format(dataset=dataset)).read_text(encoding="utf-8"))
        if err := check_anchor(f"whole-d{depth}@{K_ANCHOR}.{dataset} vs {want['config']}", got, want):
            problems.append(err)
    return problems


# ── the table and the decision (research D9) ────────────────────────────────────────────────


def load_scores() -> dict[tuple[str, int, int, str], dict]:
    out = {}
    for p in sorted(RUNS_DIR.glob("*.json")):
        if p.name.endswith(".meta.json") or p.name == "build-records.json":
            continue
        rep = json.loads(p.read_text(encoding="utf-8"))
        cell = rep.get("cell") or p.stem
        head, dataset = cell.rsplit(".", 1)
        variant, rest = head.rsplit("-d", 1)
        depth, k = rest.split("@")
        out[(variant, int(depth), int(k), dataset)] = rep
    return out


def build_records() -> dict[str, dict]:
    out = {}
    for p in sorted(STUDY_DIR.glob("*/build.json")):
        rec = json.loads(p.read_text(encoding="utf-8"))
        if rec.get("limit"):
            continue
        out[f"{rec['variant']}.{rec['dataset']}"] = rec
    return out


def decision_rows(scores) -> dict[tuple[str, str], dict]:
    """(variant, dataset) → the re-ranked @300 metrics, the rule's inputs."""
    rows = {}
    for (variant, depth, k, dataset), rep in scores.items():
        if depth == 20 and k == K_STUDY:
            rows[(variant, dataset)] = {"ndcg_10": rep["mean_ndcg_10"], "recall_100": rep["mean_recall_100"]}
    return rows


def decide(rows: dict[tuple[str, str], dict]) -> dict:
    whole = {ds: m for (v, ds), m in rows.items() if v == "whole"}
    variants = {}
    for v in CHUNKERS:
        datasets = sorted(ds for (vv, ds) in rows if vv == v and ds in whole)
        if not datasets:
            continue
        mean = statistics.mean(rows[(v, ds)]["ndcg_10"] for ds in datasets)
        whole_mean = statistics.mean(whole[ds]["ndcg_10"] for ds in datasets)
        deltas = {ds: rows[(v, ds)]["ndcg_10"] - whole[ds]["ndcg_10"] for ds in datasets}
        delta_recall = statistics.mean(rows[(v, ds)]["recall_100"] for ds in datasets) - statistics.mean(whole[ds]["recall_100"] for ds in datasets)
        recommended = (
            mean - whole_mean >= MEAN_GAIN - 1e-12
            and all(d >= -MAX_DROP - 1e-12 for d in deltas.values())
            and delta_recall >= -RECALL_DROP - 1e-12
        )
        variants[v] = {
            "datasets": datasets,
            "scope": "three-way" if len(datasets) == 3 else "two-way" if len(datasets) == 2 else f"{len(datasets)}-way",
            "mean_ndcg_10": mean,
            "whole_mean_ndcg_10": whole_mean,
            "delta_mean": mean - whole_mean,
            "deltas": deltas,
            "delta_recall": delta_recall,
            "recommended": recommended,
        }
    best = None
    if variants:
        # the best by the rule's own measure — the mean gain — ties to the non-neural chunker
        order = sorted(variants, key=lambda v: (-round(variants[v]["delta_mean"], 9), v != "contract", v))
        best = order[0]
    return {
        "variants": variants,
        "best_chunker": best,
        "constants": {"mean_gain": MEAN_GAIN, "max_drop": MAX_DROP, "recall_drop": RECALL_DROP, "min_positions": MIN_POSITIONS, "window": WINDOW},
        "rule": "recommended iff re-ranked mean nDCG@10 ≥ whole + mean_gain over the datasets run, no dataset > max_drop below whole, mean Recall@100 ≥ whole − recall_drop; best chunker by mean gain, ties to contract",
    }


def table_markdown(scores, records) -> str:
    lines = []
    for dataset in DATASETS:
        cells = {key: rep for key, rep in scores.items() if key[3] == dataset}
        if not cells:
            continue
        lines.append(f"\n### {dataset}\n")
        lines.append("| variant | k | passages | over window | under 16 | fused nDCG@10 | Δ | re-ranked nDCG@10 | Δ | fused R@100 | re-ranked R@100 | Δ | short queries (re-ranked) |")
        lines.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        base_f, base_r = cells.get(("whole", 0, K_STUDY, dataset)), cells.get(("whole", 20, K_STUDY, dataset))
        for variant in VARIANTS:
            for k in ((K_ANCHOR, K_STUDY) if variant == "whole" else (K_STUDY,)):
                f, r = cells.get((variant, 0, k, dataset)), cells.get((variant, 20, k, dataset))
                if f is None and r is None:
                    continue
                rec = records.get(f"{variant}.{dataset}", {})
                is_base = variant == "whole" or k != K_STUDY

                def delta(rep, base, metric):
                    if rep is None or base is None or is_base:
                        return ""
                    return f"{rep[metric] - base[metric]:+.4f}"

                def val(rep, metric):
                    return f"{rep[metric]:.4f}" if rep else ""

                row = [
                    variant, str(k), str(rec.get("passages", "")), str(rec.get("over_window", "")), str(rec.get("under_16", "")),
                    val(f, "mean_ndcg_10"), delta(f, base_f, "mean_ndcg_10"),
                    val(r, "mean_ndcg_10"), delta(r, base_r, "mean_ndcg_10"),
                    val(f, "mean_recall_100"), val(r, "mean_recall_100"), delta(r, base_r, "mean_recall_100"),
                    str(r.get("short_queries", "")) if r else "",
                ]
                lines.append("| " + " | ".join(row) + " |")
        a20 = cells.get(("whole", 20, K_ANCHOR, dataset))
        if a20 and base_r:
            lines.append(
                f"\nwhole@100 → whole@300 (the depth alone): re-ranked nDCG@10 {a20['mean_ndcg_10']:.4f} → {base_r['mean_ndcg_10']:.4f} "
                f"({base_r['mean_ndcg_10'] - a20['mean_ndcg_10']:+.4f}); Recall@100 {a20['mean_recall_100']:.4f} → {base_r['mean_recall_100']:.4f} "
                f"({base_r['mean_recall_100'] - a20['mean_recall_100']:+.4f})"
            )
    return "\n".join(lines) + "\n"


# ── commands ────────────────────────────────────────────────────────────────────────────────


def cmd_build(args) -> int:
    rec = build(args.variant, args.dataset, args.limit)
    print(json.dumps(rec))
    return 0


def cmd_search(args) -> int:
    path = search(args.variant, args.dataset, args.depth, args.k, args.limit)
    print(f"run: {path}")
    return 0


def cmd_score(args) -> int:
    variants = (args.variant,) if args.variant else VARIANTS
    for rep in score_dataset(args.dataset, variants):
        print(f"{rep['cell']}: nDCG@10 {rep['mean_ndcg_10']:.6f} · Recall@100 {rep['mean_recall_100']:.6f} · {rep['scored_queries']} queries · short {rep.get('short_queries')}")
    return 0


def cmd_check(args) -> int:
    problems = check(args.dataset)
    if problems:
        for p in problems:
            print(f"FAIL {p}")
        return 1
    print(f"PASS whole@{K_ANCHOR}.{args.dataset} reproduces hybrid-baseline-v2 (fused) and hybrid-rerank-v3 (re-ranked)")
    return 0


def cmd_table(args) -> int:
    print(table_markdown(load_scores(), build_records()))
    return 0


def cmd_decide(args) -> int:
    d = decide(decision_rows(load_scores()))
    for v, info in d["variants"].items():
        deltas = " ".join(f"{ds} {x:+.4f}" for ds, x in info["deltas"].items())
        print(f"{v:15s} {info['scope']:9s} mean {info['mean_ndcg_10']:.4f} vs whole {info['whole_mean_ndcg_10']:.4f} ({info['delta_mean']:+.4f}) · {deltas} · recall {info['delta_recall']:+.4f} → {'RECOMMENDED' if info['recommended'] else 'not recommended'}")
    print(f"best chunker: {d['best_chunker']}")
    if args.owner_decision:
        Path(args.owner_decision).write_text(json.dumps(d, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {args.owner_decision}")
    return 0


def cmd_all(args) -> int:
    variants = tuple(args.variant.split(",")) if args.variant else VARIANTS
    dataset = args.dataset
    if "whole" in variants:
        build("whole", dataset)
        for depth in DEPTHS:
            search("whole", dataset, depth, K_ANCHOR)
        score_dataset(dataset, ("whole",))
        problems = check(dataset)
        if problems:
            for p in problems:
                print(f"FAIL {p}")
            return 1
        print(f"PASS anchor {dataset}")
    for variant in variants:
        build(variant, dataset)
        for depth in DEPTHS:
            search(variant, dataset, depth, K_STUDY)
    for rep in score_dataset(dataset, variants):
        print(f"{rep['cell']}: nDCG@10 {rep['mean_ndcg_10']:.6f} · Recall@100 {rep['mean_recall_100']:.6f}")
    records = build_records()
    RUNS_DIR.mkdir(parents=True, exist_ok=True)
    (RUNS_DIR / "build-records.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="The chunking study (Feature 022).")
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build"); b.add_argument("--variant", required=True, choices=VARIANTS); b.add_argument("--dataset", required=True, choices=DATASETS); b.add_argument("--limit", type=int); b.set_defaults(fn=cmd_build)
    s = sub.add_parser("search"); s.add_argument("--variant", required=True, choices=VARIANTS); s.add_argument("--dataset", required=True, choices=DATASETS); s.add_argument("--depth", type=int, required=True, choices=DEPTHS); s.add_argument("--k", type=int, required=True, choices=(K_ANCHOR, K_STUDY)); s.add_argument("--limit", type=int); s.set_defaults(fn=cmd_search)
    c = sub.add_parser("score"); c.add_argument("--dataset", required=True, choices=DATASETS); c.add_argument("--variant", choices=VARIANTS); c.set_defaults(fn=cmd_score)
    k = sub.add_parser("check"); k.add_argument("--dataset", required=True, choices=DATASETS); k.set_defaults(fn=cmd_check)
    t = sub.add_parser("table"); t.set_defaults(fn=cmd_table)
    d = sub.add_parser("decide"); d.add_argument("--owner-decision"); d.set_defaults(fn=cmd_decide)
    a = sub.add_parser("all"); a.add_argument("--dataset", required=True, choices=DATASETS); a.add_argument("--variant", help="comma-separated subset of the variants"); a.set_defaults(fn=cmd_all)
    args = ap.parse_args(argv)
    try:
        return args.fn(args)
    except StudyError as e:
        print(f"chunking_study: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
