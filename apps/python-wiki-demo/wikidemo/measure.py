"""``wikidemo measure``: the twenty measurement queries at re-rank depths 0 / 5 / 10 / 20,
compared with the host goldens by the device test's rule (research D13) — or, with
``--against``, with a second artefact's live responses (research D14) — and a record with
this machine's latency per depth and its footprint (contracts/records.md).
"""

from __future__ import annotations

import json
import statistics
import struct
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import xtriever

from . import DEFAULT_DEPTH, DEPTHS
from .inputs import Paths, repo_root
from .record import dir_bytes, machine_name, now_rfc3339, os_name, peak_resident_bytes, stamp, threads, write_json
from .search import Opened, open_artefact

TOLERANCE_ABS = 1e-3
CEILING_BYTES = 600_000_000
ENGINE_DEFAULT_DEPTH = 20
K = 10
FEATURE = "019-python-wiki-demo"


# ------------------------------------------------------------------------------ truths


@dataclass(frozen=True)
class TruthHit:
    """One hit in the goldens' shape: ids and the bits of every score."""

    external_id: str
    score_bits: str
    bm25_score_bits: str | None
    dense_score_bits: str | None
    rerank_score_bits: str | None
    rerank_rank: int | None
    rerank_combined_bits: str | None


def f64_bits(x: float) -> str:
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


def f32_bits(x: float | None) -> str | None:
    return None if x is None else "%08x" % struct.unpack("<I", struct.pack("<f", x))[0]


def f32_from_bits(bits: str) -> float:
    return struct.unpack("<f", struct.pack("<I", int(bits, 16)))[0]


def truth_from_hits(hits) -> list[TruthHit]:
    """A live response's hits in the goldens' shape (the `--against` side)."""
    out = []
    for h in hits:
        e = h.explain
        out.append(
            TruthHit(
                external_id=h.external_id,
                score_bits=f64_bits(h.score),
                bm25_score_bits=None if e is None else f32_bits(e.bm25_score),
                dense_score_bits=None if e is None else f32_bits(e.dense_score),
                rerank_score_bits=f32_bits(h.rerank_score),
                rerank_rank=None if e is None else e.rerank_rank,
                rerank_combined_bits=None if e is None or e.rerank_combined is None else f64_bits(e.rerank_combined),
            )
        )
    return out


def load_truth(path: Path) -> tuple[dict[str, dict[int, list[TruthHit]]], dict[str, str]]:
    """The host goldens (`wiki expected`): per query id and depth the hits, and the query texts."""
    data = json.loads(path.read_text(encoding="utf-8"))
    truth: dict[str, dict[int, list[TruthHit]]] = {}
    texts: dict[str, str] = {}
    for q in data["queries"]:
        texts[q["id"]] = q["text"]
        truth[q["id"]] = {
            int(depth): [TruthHit(**{k: h.get(k) for k in TruthHit.__dataclass_fields__}) for h in entry["hits"]]
            for depth, entry in q["depths"].items()
        }
    return truth, texts


# -------------------------------------------------------------------------- comparison


@dataclass
class Comparison:
    queries_compared: int = 0
    lexical_bit_identical: int = 0
    fused_order_identical: int = 0
    dense_max_abs_diff: float = 0.0
    rerank_max_abs_diff: float = 0.0
    all_bits_identical: int = 0
    hits_compared: int = 0
    incomplete: list[str] = field(default_factory=list)
    tolerance_abs: float = TOLERANCE_ABS
    verdict: str = "FAIL"

    def as_record(self) -> dict:
        return {
            "queriesCompared": self.queries_compared,
            "lexicalBitIdentical": self.lexical_bit_identical,
            "fusedOrderIdentical": self.fused_order_identical,
            "denseMaxAbsDiff": self.dense_max_abs_diff,
            "rerankMaxAbsDiff": self.rerank_max_abs_diff,
            "allBitsIdentical": self.all_bits_identical,
            "hitsCompared": self.hits_compared,
            "toleranceAbs": self.tolerance_abs,
            "verdict": self.verdict,
        }


def compare(truth: dict[str, dict[int, list[TruthHit]]], responses: dict[str, dict[int, list]], depths, order_at_every_depth: bool = False) -> Comparison:
    """The device test's rule (`DeviceMeasurementTests`, research D13): every golden query,
    depth and hit must have a counterpart; lexical bits exact; fused order identical at
    depth 0; dense and re-rank scores within 1e-3 per document matched by id; a score present
    on one side only is a mismatch, not a skip.

    The device rule checks order only at depth 0 because a drift within tolerance may swap
    re-ranked neighbours. Two builds on one host have no drift to tolerate, so the slice
    check (`--against`, spec FR-014) sets `order_at_every_depth`: the ids must then be in the
    same order at every depth, counted in `fused_order_identical` per query."""
    c = Comparison()
    measured = set(depths)
    for qid, by_depth in truth.items():
        got_by_depth = responses.get(qid)
        if got_by_depth is None:
            c.incomplete.append(f"{qid}: not searched")
            continue
        c.queries_compared += 1
        if set(by_depth) != measured:
            c.incomplete.append(f"{qid}: goldens carry depths {sorted(by_depth)}, the harness measures {sorted(measured)}")
        lexical_identical = True
        fused_identical = True
        for depth, want in by_depth.items():
            got = got_by_depth.get(depth)
            if got is None:
                c.incomplete.append(f"{qid}@{depth}: no response")
                continue
            if len(got) != len(want):
                c.incomplete.append(f"{qid}@{depth}: {len(got)} hits, host has {len(want)}")
            if depth == 0 or order_at_every_depth:
                fused_identical = fused_identical and [h.external_id for h in got] == [w.external_id for w in want]
            want_by_id = {}
            for w in want:
                want_by_id.setdefault(w.external_id, w)
            for g in got:
                w = want_by_id.get(g.external_id)
                if w is None:
                    c.incomplete.append(f"{qid}@{depth}: {g.external_id} is not among the host's hits")
                    continue
                c.hits_compared += 1
                e = g.explain
                g_bm25 = None if e is None else f32_bits(e.bm25_score)
                if g_bm25 != w.bm25_score_bits:
                    lexical_identical = False
                g_dense = None if e is None else e.dense_score
                if w.dense_score_bits is not None and g_dense is not None:
                    c.dense_max_abs_diff = max(c.dense_max_abs_diff, abs(g_dense - f32_from_bits(w.dense_score_bits)))
                elif (w.dense_score_bits is None) != (g_dense is None):
                    c.incomplete.append(f"{qid}@{depth} {g.external_id}: dense score present on one side only")
                if w.rerank_score_bits is not None and g.rerank_score is not None:
                    c.rerank_max_abs_diff = max(c.rerank_max_abs_diff, abs(g.rerank_score - f32_from_bits(w.rerank_score_bits)))
                elif (w.rerank_score_bits is None) != (g.rerank_score is None):
                    c.incomplete.append(f"{qid}@{depth} {g.external_id}: re-rank score present on one side only")
                if truth_from_hits([g])[0] == w:
                    c.all_bits_identical += 1
        if lexical_identical:
            c.lexical_bit_identical += 1
        if fused_identical:
            c.fused_order_identical += 1
    ok = (
        c.queries_compared > 0
        and c.queries_compared == len(truth)
        and not c.incomplete
        and c.lexical_bit_identical == c.queries_compared
        and c.fused_order_identical == c.queries_compared
        and c.dense_max_abs_diff <= c.tolerance_abs
        and c.rerank_max_abs_diff <= c.tolerance_abs
    )
    c.verdict = "PASS" if ok else "FAIL"
    return c


# ------------------------------------------------------------------------- the runs


@dataclass
class QueryRun:
    id: str
    depth: int
    elapsed_ms: int
    engine_ms: int
    hits: int
    peak_bytes_after: int

    def as_record(self) -> dict:
        return {
            "id": self.id,
            "depth": self.depth,
            "elapsedMs": self.elapsed_ms,
            "engineMs": self.engine_ms,
            "hits": self.hits,
            "peakBytesAfter": self.peak_bytes_after,
        }


def run_queries(opened: Opened, queries: list[dict], depths=DEPTHS, k: int = K):
    """One labelled warm-up (the first query at depth 0, not counted), then every query at
    every depth. Returns (runs, responses keyed by id and depth, warm-up ms)."""
    handle = opened.handle
    t = time.perf_counter()
    handle.search(queries[0]["text"], xtriever.SearchOptions(k=k, rerank_depth=0, explain=True))
    warmup_ms = round((time.perf_counter() - t) * 1000)
    runs: list[QueryRun] = []
    responses: dict[str, dict[int, list]] = {}
    for q in queries:
        responses[q["id"]] = {}
        for depth in depths:
            t = time.perf_counter()
            r = handle.search(q["text"], xtriever.SearchOptions(k=k, rerank_depth=depth, explain=True))
            elapsed = round((time.perf_counter() - t) * 1000)
            responses[q["id"]][depth] = r.hits
            runs.append(QueryRun(q["id"], depth, elapsed, r.elapsed_ms, len(r.hits), peak_resident_bytes()))
            print(f"  {q['id']} depth {depth:2d}: {elapsed} ms, {len(r.hits)} hits", file=sys.stderr)
    return runs, responses, warmup_ms


def _median(values):
    return statistics.median(values) if values else None


def summarise(runs: list[QueryRun], depths=DEPTHS) -> dict:
    by_depth = {d: [r.elapsed_ms for r in runs if r.depth == d] for d in depths}
    per_query_total = {}
    for r in runs:
        if r.depth in (0, DEFAULT_DEPTH):
            per_query_total[r.id] = per_query_total.get(r.id, 0) + r.elapsed_ms
    totals = list(per_query_total.values())
    return {
        "perDepthMedianMs": {str(d): _median(v) for d, v in by_depth.items()},
        "perDepthMaxMs": {str(d): (max(v) if v else None) for d, v in by_depth.items()},
        "latency": {
            "medianFusedMs": _median(by_depth.get(0, [])),
            "maxFusedMs": max(by_depth.get(0, []), default=None),
            "medianRerankedMs": _median(by_depth.get(DEFAULT_DEPTH, [])),
            "maxRerankedMs": max(by_depth.get(DEFAULT_DEPTH, []), default=None),
            "medianRerankedAtEngineDefaultMs": _median(by_depth.get(ENGINE_DEFAULT_DEPTH, [])),
            "medianTotalMs": _median(totals),
            "maxTotalMs": max(totals, default=None),
        },
    }


def make_record(*, corpus, index_meta, open_ms, embedder_load_ms, reranker_load_ms, warmup_ms, runs, depths, comparison, peak_bytes, against=None, notes=()) -> dict:
    n_threads, source = threads()
    record = {
        "schemaVersion": 1,
        "feature": FEATURE,
        "corpus": corpus,
        "machine": machine_name(),
        "os": os_name(),
        "python": sys.version.split()[0],
        "xtrieverVersion": xtriever.__version__,
        "build": {"loadPath": "mmap", "effectiveThreads": n_threads, "threadSource": source},
        "index": index_meta,
        "openMs": open_ms,
        "embedderLoadMs": embedder_load_ms,
        "rerankerLoadMs": reranker_load_ms,
        "warmupMs": warmup_ms,
        "queries": [r.as_record() for r in runs],
        **summarise(runs, depths),
        "footprint": {
            "peakBytes": peak_bytes,
            "peakMethod": "ru_maxrss",
            "ceilingBytes": CEILING_BYTES,
            "underCeiling": peak_bytes <= CEILING_BYTES,
        },
        "parity": comparison.as_record(),
        "notes": list(notes),
        "recordedAt": now_rfc3339(),
    }
    if against is not None:
        record["against"] = against
    return record


def index_meta(opened: Opened, sidecar: dict | None) -> dict:
    info = opened.info
    return {
        "bytes": dir_bytes(opened.paths.index_dir),
        "documents": info.documents,
        "formatVersion": info.format_version,
        "embedderFingerprint": info.embedder_fingerprint,
        "rerankerModelId": info.reranker_model_id,
        "corpusIdentity": None if sidecar is None else sidecar.get("corpus_identity"),
        "partial": None if sidecar is None else sidecar.get("partial"),
    }


def default_record_path(slice_mode: bool) -> Path:
    n_threads, source = threads()
    suffix = f"threads{n_threads}" if source == "RAYON_NUM_THREADS" else "threadsdefault"
    prefix = "slice-" if slice_mode else ""
    return repo_root() / "specs/019-python-wiki-demo/runs" / f"{prefix}{machine_name()}-{stamp()}-mmap-{suffix}.json"


def print_summary(record: dict) -> None:
    lat = record["latency"]
    print("per-depth median ms: " + " · ".join(f"depth {d} {v}" for d, v in record["perDepthMedianMs"].items()))
    print("per-depth max ms:    " + " · ".join(f"depth {d} {v}" for d, v in record["perDepthMaxMs"].items()))
    print(f"latency: fused {lat['medianFusedMs']} ms · re-ranked (depth {DEFAULT_DEPTH}) {lat['medianRerankedMs']} ms · total {lat['medianTotalMs']} ms · re-ranked (depth {ENGINE_DEFAULT_DEPTH}) {lat['medianRerankedAtEngineDefaultMs']} ms (medians)")
    fp = record["footprint"]
    print(f"footprint: peak resident {fp['peakBytes'] // (1024 * 1024)} MB ({fp['peakMethod']}); the phone's ceiling is {fp['ceilingBytes'] // 1_000_000} MB, for comparison only")
    p = record["parity"]
    print(
        f"parity: {p['verdict']} ({p['queriesCompared']} queries; lexical bit-identical {p['lexicalBitIdentical']}, "
        f"fused order identical {p['fusedOrderIdentical']}, dense max |Δ| {p['denseMaxAbsDiff']:g}, re-rank max |Δ| {p['rerankMaxAbsDiff']:g}, "
        f"all bits identical {p['allBitsIdentical']}/{p['hitsCompared']})"
    )


def run_measure(args, paths: Paths) -> int:
    from .about import read_sidecar

    queries = json.loads(paths.queries.read_text(encoding="utf-8"))
    sidecar = read_sidecar(paths)
    against_paths = None
    if args.against:
        from .inputs import resolve

        against_paths = resolve(type("A", (), {"artefact": args.against, "embedder": args.embedder, "reranker": args.reranker})())
        if not (against_paths.index_dir / "xtriever-pipeline.json").exists():
            print(f"wikidemo: --against {args.against}: no index at {against_paths.index_dir}", file=sys.stderr)
            return 1
    elif sidecar is not None and sidecar.get("partial") is not None:
        print("wikidemo: the host goldens describe the full corpus; compare a slice with --against", file=sys.stderr)
        return 1

    opened = open_artefact(paths)
    print(f"opened {paths.artefact} · open {opened.open_ms} ms", file=sys.stderr)
    runs, responses, warmup_ms = run_queries(opened, queries)

    against = None
    if against_paths is None:
        truth, _texts = load_truth(paths.expected)
        corpus = "wikipedia"
    else:
        other = open_artefact(against_paths)
        print(f"opened --against {against_paths.artefact} · open {other.open_ms} ms", file=sys.stderr)
        truth = {}
        for q in queries:
            truth[q["id"]] = {}
            for depth in DEPTHS:
                r = other.handle.search(q["text"], xtriever.SearchOptions(k=K, rerank_depth=depth, explain=True))
                truth[q["id"]][depth] = truth_from_hits(r.hits)
        other_sidecar = read_sidecar(against_paths)
        against = {
            "artefact": str(against_paths.artefact),
            "corpusIdentity": None if other_sidecar is None else other_sidecar.get("corpus_identity"),
            "identityEqual": (sidecar or {}).get("corpus_identity") == (other_sidecar or {}).get("corpus_identity"),
            "counts": None if other_sidecar is None else other_sidecar.get("counts"),
            "countsEqual": (sidecar or {}).get("counts") == (other_sidecar or {}).get("counts"),
            "documents": other.info.documents,
            "documentsEqual": other.info.documents == opened.info.documents,
        }
        corpus = "wikipedia-slice"

    comparison = compare(truth, responses, DEPTHS, order_at_every_depth=against is not None)
    if against is not None:
        # A slice is the same slice only if the builds agree on what they built, too.
        against["verdict"] = "PASS" if against["identityEqual"] and against["countsEqual"] and against["documentsEqual"] else "FAIL"
    record = make_record(
        corpus=corpus,
        index_meta=index_meta(opened, sidecar),
        open_ms=opened.open_ms,
        embedder_load_ms=opened.info.embedder_load_ms,
        reranker_load_ms=opened.info.reranker_load_ms,
        warmup_ms=warmup_ms,
        runs=runs,
        depths=DEPTHS,
        comparison=comparison,
        peak_bytes=peak_resident_bytes(),
        against=against,
        notes=[f"parity: {m}" for m in comparison.incomplete],
    )
    out = Path(args.out) if args.out else default_record_path(against is not None)
    write_json(out, record)
    print_summary(record)
    if against is not None:
        print(f"against: {against['verdict']} (identity equal {against['identityEqual']} · counts equal {against['countsEqual']} · documents equal {against['documentsEqual']}; order checked at every depth)")
    print(f"wrote {out}")
    ok = comparison.verdict == "PASS" and (against is None or against["verdict"] == "PASS")
    return 0 if ok else 1
