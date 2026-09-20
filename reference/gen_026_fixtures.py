#!/usr/bin/env python3
"""Feature 026 goldens: the dense stage's expectations under eight-bit storage.

Principle II says a golden comes from `reference/`, not from the implementation it checks. This
recomputes, in Python, every expectation the dense stage's tests replay, under the scheme
ADR-0015 fixes: one scale per vector (`max|component| / 127`), codes rounded and clamped to
±127, the dot product accumulated exactly in integers and multiplied by the two scales once.

It rewrites, in place:

* `reference/fixtures/004/search.json` — every case's expected hits and scores;
* `reference/fixtures/004/mutations.json` — the same, after each scripted mutation;
* `reference/fixtures/005/hybrid.json` — the pipeline's expected dense scores;
* `crates/xtriever-dense/tests/support/v3_oracle.json` — the scripted sequences that were
  minted on format 1 and replayed unchanged through format 2. **Format 3 changes scores by
  design**, so the file is re-minted rather than pretended to be unchanged, and it is renamed to
  say which format it describes.

    python3 reference/gen_026_fixtures.py            # check: recompute and report mismatches
    python3 reference/gen_026_fixtures.py --write    # rewrite the goldens
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "reference/fixtures/004"
PIPELINE = ROOT / "reference/fixtures/005/hybrid.json"
ORACLE_IN = ROOT / "crates/xtriever-dense/tests/support/v1_oracle.json"
ORACLE_OUT = ROOT / "crates/xtriever-dense/tests/support/v3_oracle.json"


def f32(x: float) -> float:
    """Round to `f32`, the width the stage stores and returns."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def quantise(vector: list[float]) -> tuple[list[int], float]:
    """The scheme, restated: symmetric, one scale per vector, never code −128."""
    peak = max((abs(x) for x in vector), default=0.0)
    scale = f32(peak / 127.0) if peak > 0.0 else 1.0
    codes = []
    for x in vector:
        code = round(x / scale)
        codes.append(max(-127, min(127, code)))
    return codes, scale


def recovered(vector: list[float]) -> list[float]:
    codes, scale = quantise(vector)
    return [f32(c * scale) for c in codes]


def norm_f64(v: list[float]) -> float:
    return math.sqrt(sum(x * x for x in v))


def score(metric: str, q: list[float], q_norm: float, row: list[float], row_norm_f32: float) -> float:
    """One row's score, over what the stage stores rather than what was added."""
    if metric == "euclidean":
        # A distance is not a dot product: it works on the recovered components.
        acc = 0.0
        for a, b in zip(q, recovered(row)):
            d = a - b
            acc += d * d
        return f32(-math.sqrt(acc))
    q_codes, q_scale = quantise(q)
    row_codes, row_scale = quantise(row)
    accumulator = sum(a * b for a, b in zip(q_codes, row_codes))   # exact, in integers
    dot = accumulator * q_scale * row_scale
    if metric == "dot":
        return f32(dot)
    if metric == "cosine":
        return f32(dot / (q_norm * row_norm_f32))
    raise ValueError(metric)


def ranked(scores: dict[int, float], allowed: set[int] | None) -> list[tuple[int, float]]:
    """`(score DESC, id ASC)`, the stage's total order."""
    items = [(i, s) for i, s in scores.items() if allowed is None or i in allowed]
    items.sort(key=lambda pair: (-pair[1], pair[0]))
    return items


def rescore_search(document: dict) -> int:
    """Recompute `search.json`'s expectations. Returns how many cases changed."""
    changed = 0
    for group in document["sets"]:
        metric = group["metric"]
        rows = {row["id"]: [f32(x) for x in row["vector"]] for row in group["rows"]}
        norms = {i: f32(norm_f64(v)) for i, v in rows.items()}
        for query in group["queries"]:
            q = [f32(x) for x in query["vector"]]
            q_norm = norm_f64(q)
            scores = {i: score(metric, q, q_norm, v, norms[i]) for i, v in rows.items()}
            for case in query["cases"]:
                allowed = None if case["allowed"] is None else set(case["allowed"])
                if case["k"] == 0 or allowed == set():
                    expected = []
                else:
                    expected = [
                        {"id": i, "score": s} for i, s in ranked(scores, allowed)[: case["k"]]
                    ]
                if expected != case["expected"]:
                    changed += 1
                    case["expected"] = expected
    return changed


def rescore_mutations(document: dict) -> int:
    """`mutations.json` is one scripted stream of `add` / `delete` / `commit` / `expect` steps;
    each `expect` names a query and the results the stage must return at that point."""
    metric = document["metric"]
    live: dict[int, list[float]] = {}
    committed: dict[int, list[float]] = {}
    changed = 0
    for step in document["steps"]:
        op = step["op"]
        if op == "add":
            live[step["id"]] = [f32(x) for x in step["vector"]]
        elif op == "delete":
            for doomed in step["ids"]:
                live.pop(doomed, None)
        elif op == "commit":
            committed = dict(live)
        elif op == "reopen":
            # A reopen drops whatever was staged and never committed (step 19's add is gone by
            # step 21's expect, which is why the script has it).
            live = dict(committed)
        elif op == "expect":
            q = [f32(x) for x in step["query"]]
            q_norm = norm_f64(q)
            norms = {i: f32(norm_f64(v)) for i, v in committed.items()}
            scores = {i: score(metric, q, q_norm, v, norms[i]) for i, v in committed.items()}
            results = [
                {"id": i, "score": s} for i, s in ranked(scores, None)[: step["k"]]
            ]
            if results != step["results"]:
                changed += 1
                step["results"] = results
            if len(committed) != step["len"]:
                raise SystemExit(
                    f"mutations.json: len {step['len']} but the model holds {len(committed)}"
                )
    return changed


def rescore_pipeline(document: dict) -> int:
    """The pipeline fixture's expected dense scores, recomputed under format 3.

    The hybrid fixture carries its own document vectors and a query vector per case, and the
    dense stage there is cosine over unit-length vectors. Fusion is untouched: reciprocal rank
    fusion reads ranks, not scores, so the fused expectations move only if a rank moves — and
    the test asserts that too, which is the point of recomputing rather than loosening.
    """
    rows = {doc["external_id"]: [f32(x) for x in doc["vector"]] for doc in document["documents"]}
    norms = {i: f32(norm_f64(v)) for i, v in rows.items()}
    changed = 0
    for query in document["queries"]:
        q = [f32(x) for x in query["vector"]]
        q_norm = norm_f64(q)
        scores = {i: score("cosine", q, q_norm, v, norms[i]) for i, v in rows.items()}
        wanted = [entry["id"] for entry in query["expected_dense"]]
        recomputed = [
            {"id": i, "score": s} for i, s in ranked(scores, set(wanted))[: len(wanted)]
        ]
        if recomputed != query["expected_dense"]:
            changed += 1
            query["expected_dense"] = recomputed
    return changed


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def refresh_manifest() -> None:
    """Keep `manifest.json` honest: the files' hashes, and which generator recomputed them.

    `generator_sha256` still names the Feature 004 generator that produced the rows, the queries
    and the mutation script; `rescored_by` names this file, which recomputed the expectations
    under format 3. Neither claim is the other's.
    """
    path = FIXTURES / "manifest.json"
    manifest = json.loads(path.read_text(encoding="utf-8"))
    for name in manifest["files"]:
        manifest["files"][name] = sha256_file(FIXTURES / name)
    manifest["rescored_by"] = "reference/gen_026_fixtures.py"
    manifest["rescored_by_sha256"] = sha256_file(Path(__file__))
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"  manifest.json: hashes refreshed, rescored_by recorded")


def check_oracle(path: Path) -> int:
    """Recompute every expectation in the scripted oracle and report mismatches.

    The file is minted by the crate's own `mint` test; this is what keeps it honest, by
    recomputing the same numbers from the contract's arithmetic in Python. Zero mismatches means
    the crate and this file agree about what format 3 does.
    """
    oracle = json.loads(path.read_text(encoding="utf-8"))
    mismatches = 0
    checked = 0
    for sequence in oracle["sequences"]:
        metric = sequence["metric"]
        live: dict[int, list[float]] = {}
        committed: dict[int, list[float]] = {}
        for step in sequence["steps"]:
            op = step["op"]
            if op in ("add", "replace"):
                live[step["id"]] = [f32(x) for x in step["vector"]]
            elif op == "delete":
                for doomed in step.get("ids", [step.get("id")]):
                    live.pop(doomed, None)
            elif op == "commit":
                committed = dict(live)
            elif op == "reopen":
                live = dict(committed)          # staged changes do not survive a reopen
            elif op == "expect":
                if len(committed) != step["len"]:
                    raise SystemExit(
                        f"{path.name}: expected {step['len']} live rows, model holds {len(committed)}"
                    )
                norms = {i: f32(norm_f64(v)) for i, v in committed.items()}
                for query in step["queries"]:
                    q = [f32(x) for x in query["vector"]]
                    q_norm = norm_f64(q)
                    scores = {i: score(metric, q, q_norm, v, norms[i]) for i, v in committed.items()}
                    allowed = None if query.get("allowed") is None else set(query["allowed"])
                    hits = [
                        [i, struct.unpack("<I", struct.pack("<f", s))[0]]
                        for i, s in ranked(scores, allowed)[: query["k"]]
                    ]
                    checked += 1
                    if hits != query["hits"]:
                        mismatches += 1
    print(f"  {path.name}: {checked} queries recomputed, {mismatches} mismatches")
    return mismatches


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--write", action="store_true", help="rewrite the goldens in place")
    parser.add_argument("--check-oracle", action="store_true", help="recompute the scripted oracle")
    args = parser.parse_args()

    if args.check_oracle:
        return 1 if check_oracle(ORACLE_OUT) else 0

    total = 0
    if PIPELINE.exists():
        pipeline = json.loads(PIPELINE.read_text(encoding="utf-8"))
        changed = rescore_pipeline(pipeline)
        total += changed
        print(f"  005/hybrid.json: {changed} queries recomputed")
        if args.write and changed:
            PIPELINE.write_text(json.dumps(pipeline, indent=2) + "\n", encoding="utf-8")
    for name, rescore in (("search.json", rescore_search), ("mutations.json", rescore_mutations)):
        path = FIXTURES / name
        if not path.exists():
            print(f"  {name}: missing", file=sys.stderr)
            continue
        document = json.loads(path.read_text(encoding="utf-8"))
        changed = rescore(document)
        total += changed
        print(f"  {name}: {changed} expectations recomputed")
        if args.write and changed:
            path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    if args.write:
        refresh_manifest()
    else:
        print(f"{total} expectations differ from the committed goldens (run with --write)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
