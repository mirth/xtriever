#!/usr/bin/env python3
"""Generate the golden fixtures for Feature 005 (the hybrid pipeline).

Two artifacts (Principle II):

* **Fusion oracle** -- ``fusion.json``: reciprocal rank fusion over hand-made candidate lists,
  computed by the three-line ``rrf`` below (research D5). Python's ``float`` is IEEE f64 and the
  two terms are added in the same order as the Rust side, so results are bit-identical and every
  tie between documents with the same rank pair is exact.
* **Hybrid fixture** -- ``hybrid.json``: a synthetic corpus with fields, chunk provenance and 8-d
  vectors, plus queries with vectors and filters and the dense oracle (the 004 ``fsum`` cosine),
  so the pipeline tests can prove which vectors reached the dense stage. Lexical rankings are
  Feature 002's business and are not re-oracled here.

``--verify-fusion explain.jsonl`` recomputes RRF over the per-query lexical/dense lists a real
run exported and checks the fused order (quickstart Step 5).

Run via a pinned virtualenv (needs only the standard library and NumPy):

    reference/.venv-003/bin/python reference/gen_005_fixtures.py --seed 5
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
        f"gen_005_fixtures.py requires a pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh 003\n"
        "  reference/.venv-003/bin/python reference/gen_005_fixtures.py --seed 5"
    )

import numpy as np  # noqa: E402

from dense_format3 import Prepared  # noqa: E402  (dense format 3's scheme, once)
from dense_format3 import score as score_f3  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent

RRF_K = 60
SCORE_ABS_TOL = 1e-9
DIM = 8
FINGERPRINT = "table-fp"
TIE_MARGIN = 1e-6


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    shown = path.resolve()
    shown = shown.relative_to(REPO_ROOT) if shown.is_relative_to(REPO_ROOT) else shown
    print(f"  wrote {shown}")


# --------------------------------------------------------------------------------------------
# The fusion rule (research D5). Lexical term first, then dense -- the Rust side adds in the
# same order, so identical rank pairs give bit-identical sums on both sides.
# --------------------------------------------------------------------------------------------


def rrf(lexical: list, dense: list, k: int = RRF_K) -> list[tuple]:
    lex_rank = {d: r for r, d in enumerate(lexical, start=1)}
    den_rank = {d: r for r, d in enumerate(dense, start=1)}
    scores = {}
    for d in set(lex_rank) | set(den_rank):
        lex = 1.0 / (k + lex_rank[d]) if d in lex_rank else 0.0
        den = 1.0 / (k + den_rank[d]) if d in den_rank else 0.0
        scores[d] = lex + den
    return sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))


# --------------------------------------------------------------------------------------------
# fusion.json (T005)
# --------------------------------------------------------------------------------------------


def gen_fusion() -> dict:
    ids = list(range(100, 130))
    cases = [
        ("both_lists", ids[:10], ids[5:15], 10),
        ("lexical_only", ids[:8], [], 10),
        ("dense_only", [], ids[3:11], 10),
        ("disjoint", ids[:6], ids[10:16], 10),
        ("identical", ids[:7], ids[:7], 10),
        ("reversed", ids[:7], list(reversed(ids[:7])), 10),
        # ranks (2, 4) for doc 101 and (4, 2) for doc 103 tie exactly at fused ranks 2/3; with
        # k = 2 the tie sits at the k-th rank and the lower id (101) wins the last slot.
        ("tie_at_k", [ids[0], ids[1], ids[2], ids[3]], [ids[0], ids[3], ids[2], ids[1]], 2),
        ("depth_below_k", ids[:5], ids[2:7], 10),
        ("k_zero", ids[:5], ids[:5], 0),
        ("both_empty", [], [], 10),
    ]
    out = []
    for case_id, lex, den, k in cases:
        fused = rrf(lex, den)[:k]
        # Every exact tie must be between documents with identical rank pairs.
        ranks = {d: (lex.index(d) + 1 if d in lex else None, den.index(d) + 1 if d in den else None) for d in set(lex) | set(den)}
        for (a, sa), (b, sb) in zip(fused, fused[1:]):
            if sa == sb:
                assert sorted(x for x in ranks[a] if x) == sorted(x for x in ranks[b] if x), (case_id, a, b)
        out.append({"id": case_id, "lexical": lex, "dense": den, "k": k,
                    "expected": [{"id": d, "score": s} for d, s in fused]})
    tie = next(c for c in out if c["id"] == "tie_at_k")
    assert len(tie["expected"]) == 2 and tie["expected"][-1]["id"] == ids[1], tie
    return {"schema_version": 1, "rrf_k": RRF_K, "score_abs_tol": SCORE_ABS_TOL, "cases": out}


# --------------------------------------------------------------------------------------------
# hybrid.json (T006)
# --------------------------------------------------------------------------------------------

WORD_BANK = (
    "harbour lantern compass meadow granite tunnel orchard beacon quarry thicket riverbed "
    "cinder gantry parapet furrow saltmarsh kiln trellis brackish weir bitmap cursor segment "
    "posting merge commit reader writer query score filter rank fusion embed passage corpus"
).split()
# Planted terms: each query names one, so the lexical stage retrieves a known subset.
PLANTS = ["zephyr", "quasar", "obsidian", "marlin", "sextant"]


def fdot(a: np.ndarray, b: np.ndarray) -> float:
    return math.fsum(float(x) * float(y) for x, y in zip(a.tolist(), b.tolist()))


def cosine_ranking(q: np.ndarray, docs: list[dict], allowed: set[str] | None) -> list[tuple[str, float]]:
    """The dense stage's ranking **as the engine stores and scores it** (dense format 3, Feature
    026): the scheme in `dense_format3.py`, imported rather than restated. The `fsum` float
    cosine this replaced minted the fixture under formats 1 and 2."""
    prepared_q = Prepared(q.tolist())
    scored = []
    for d in docs:
        if allowed is not None and d["external_id"] not in allowed:
            continue
        scored.append((d["external_id"], score_f3("cosine", prepared_q, Prepared(d["vector"]))))
    # Ties by ascending *internal* id = ingestion order = position in `docs`.
    order = {d["external_id"]: i for i, d in enumerate(docs)}
    scored.sort(key=lambda p: (-p[1], order[p[0]]))
    for (a, sa), (b, sb) in zip(scored, scored[1:]):
        if sa != sb and abs(sa - sb) < TIE_MARGIN:
            raise ValueError(f"near-tie {sa - sb:.2e} between {a} and {b}")
        if sa == sb and docs[order[a]]["vector"] != docs[order[b]]["vector"]:
            raise ValueError(f"undesigned exact tie between {a} and {b}")
    return scored


def resolve(filter_: dict | None, docs: list[dict]) -> set[str] | None:
    if filter_ is None:
        return None
    (kind, payload), = filter_.items()
    if kind == "Eq":
        field, value = payload
        (_, val), = value.items()
        return {d["external_id"] for d in docs if d["fields"].get(field, {}).get("Keyword") == val}
    if kind == "In":
        field, values = payload
        vals = {list(v.values())[0] for v in values}
        return {d["external_id"] for d in docs if d["fields"].get(field, {}).get("Keyword") in vals}
    if kind == "Not":
        inner = resolve(payload, docs)
        return {d["external_id"] for d in docs} - inner
    raise ValueError(kind)


def gen_hybrid(seed: int) -> dict:
    for attempt in range(50):
        rng = random.Random(seed + attempt)
        nrng = np.random.default_rng(seed + attempt)
        docs = []
        n = 40
        for i in range(n):
            words = [rng.choice(WORD_BANK) for _ in range(rng.randint(8, 20))]
            plant = PLANTS[i % len(PLANTS)] if i % 3 != 2 else None
            if plant:
                words.insert(rng.randrange(len(words)), plant)
            title = " ".join(rng.choice(WORD_BANK) for _ in range(rng.randint(1, 3))).capitalize()
            text = " ".join(words)
            doc = {
                "external_id": f"d{i + 1:03d}",
                "fields": {
                    "title": {"Text": title},
                    "text": {"Text": text},
                    "source": {"Keyword": "abc"[i % 3]},
                },
                "chunk": None,
                "passage": f"{title} {text}",
                "vector": [float(np.float32(x)) for x in nrng.standard_normal(DIM)],
            }
            docs.append(doc)
        # Chunked documents: three parents with 2-3 chunks each (the last 8 docs).
        parents = [("src-1", [32, 33]), ("src-2", [34, 35, 36]), ("src-3", [37, 38, 39])]
        for parent, members in parents:
            offset = 0
            for ordinal, i in enumerate(members):
                length = len(docs[i]["fields"]["text"]["Text"])
                docs[i]["chunk"] = {"parent": parent, "ordinal": ordinal, "byte_range": [offset, offset + length]}
                offset += length + 1
        # Two documents with an empty title (the passage is then the text alone).
        for i in (5, 17):
            docs[i]["fields"]["title"] = {"Text": ""}
            docs[i]["passage"] = docs[i]["fields"]["text"]["Text"]
        # One duplicated vector so an exact dense tie is designed, not accidental.
        docs[20]["vector"] = list(docs[10]["vector"])

        filters = [
            None,
            {"Eq": ["source", {"Keyword": "a"}]},
            {"In": ["source", [{"Keyword": "a"}, {"Keyword": "b"}]]},
            {"Not": {"Eq": ["source", {"Keyword": "c"}]}},
            None, None,
            {"Eq": ["source", {"Keyword": "b"}]},
            None,
        ]
        texts = [
            "zephyr", "quasar harbour", "obsidian", "marlin sextant", "lantern compass",
            "", "quasar", "zephyr obsidian marlin",
        ]
        queries = []
        try:
            for qi, (text, flt) in enumerate(zip(texts, filters)):
                q = nrng.standard_normal(DIM).astype(np.float32)
                allowed = resolve(flt, docs)
                ranking = cosine_ranking(q, docs, allowed)[:100]
                queries.append({
                    "id": f"q{qi}",
                    "text": text,
                    "vector": [float(x) for x in q],
                    "filter": flt,
                    "expected_dense": [{"id": d, "score": s} for d, s in ranking],
                })
        except ValueError as e:
            print(f"  reroll: seed {seed + attempt} rejected -- {e}")
            continue
        schema = {"fields": [
            {"name": "title", "kind": {"Text": "standard"}, "indexed": True, "stored": False, "boost": 2.0},
            {"name": "text", "kind": {"Text": "standard"}, "indexed": True, "stored": False, "boost": 1.0},
            {"name": "source", "kind": "Keyword", "indexed": True, "stored": False, "boost": 1.0},
        ]}
        return {"schema_version": 1, "seed": seed + attempt, "dim": DIM, "fingerprint": FINGERPRINT,
                "tie_margin": TIE_MARGIN, "score_abs_tol": 1e-6, "dense_fields": ["title", "text"],
                "schema": schema, "documents": docs, "queries": queries}
    sys.exit("gen_hybrid: no unambiguous seed found")


# --------------------------------------------------------------------------------------------
# --verify-fusion (T007)
# --------------------------------------------------------------------------------------------


def rrf_from_ranks(lexical: list, dense: list, k: int = RRF_K) -> list[tuple]:
    """RRF over explicit ``[rank, id]`` pairs (an exported run carries the stages' own ranks)."""
    lex_rank = {d: r for r, d in lexical}
    den_rank = {d: r for r, d in dense}
    scores = {}
    for d in set(lex_rank) | set(den_rank):
        lex = 1.0 / (k + lex_rank[d]) if d in lex_rank else 0.0
        den = 1.0 / (k + den_rank[d]) if d in den_rank else 0.0
        scores[d] = lex + den
    return sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))


def verify_fusion(path: Path) -> int:
    """Recompute RRF over the complete stage lists a run exported (``[rank, id]`` pairs) and
    check the fused order. Ties in the oracle are broken by external id here, while the pipeline
    breaks them by internal id; a tie block is therefore compared as a set."""
    checked = bad = 0
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        rec = json.loads(line)
        expected = rrf_from_ranks(rec["lexical"], rec["dense"])
        got = rec["fused"]
        n = len(got)
        # Compare block by block: consecutive equal scores form a block whose members may be in
        # any order (the pipeline orders them by internal id, unknown here).
        ok = True
        i = 0
        while i < n and ok:
            score = expected[i][1]
            j = i
            while j < len(expected) and expected[j][1] == score:
                j += 1
            block = {d for d, _ in expected[i:j]}
            got_block = set(got[i:min(j, n)])
            if not got_block <= block:
                ok = False
            i = j
        if not ok:
            bad += 1
            print(f"  FAIL {rec['query_id']}: fused order disagrees with RRF")
        checked += 1
    print(f"verify-fusion: {checked} queries checked, {bad} disagree")
    return 1 if bad or checked == 0 else 0


def write_manifest(out: Path) -> None:
    files = sorted(p.name for p in out.iterdir() if p.suffix == ".json" and p.name != "manifest.json")
    write_json(out / "manifest.json", {
        "schema_version": 1,
        "files": {name: sha256_file(out / name) for name in files},
        "generator_sha256": sha256_file(Path(__file__)),
        "python_version": ".".join(map(str, sys.version_info[:3])),
        "scheme": "reference/dense_format3.py",
        "scheme_sha256": sha256_file(Path(__file__).resolve().parent / "dense_format3.py"),
    })


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=int, default=5)
    ap.add_argument("--out", type=Path, default=REPO_ROOT / "reference/fixtures/005")
    ap.add_argument("--refresh-manifest", action="store_true")
    ap.add_argument("--verify-fusion", type=Path, metavar="EXPLAIN_JSONL")
    args = ap.parse_args()
    if args.refresh_manifest:
        write_manifest(args.out)
        return 0
    if args.verify_fusion:
        return verify_fusion(args.verify_fusion)
    print("fusion.json")
    write_json(args.out / "fusion.json", gen_fusion())
    print("hybrid.json")
    write_json(args.out / "hybrid.json", gen_hybrid(args.seed))
    write_manifest(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
