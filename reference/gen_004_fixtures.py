#!/usr/bin/env python3
"""Generate the golden fixtures for Feature 004 (the dense stage).

Two oracles (Principle II), written by the same script so their model identity cannot drift:

* **Embedding parity** -- ``embeddings.json``: Feature 001's reference pipeline (``AutoModel`` +
  attention-mask-weighted mean pooling + L2 normalisation, the pinned torch/transformers) over a
  fixed set of texts, including the edge cases spec FR-006 names. Compared at cosine >= 0.9999 and
  max-abs <= 1e-3, after a tokenization check on the token ids (research D5, 001 D6).
* **Exact search** -- ``search.json`` / ``mutations.json``: NumPy ``float64`` scoring over
  ``float32`` vectors, ordered by ``(-score, id)``. Compared ids-and-order exact, scores within
  1e-6. Every tie in the goldens is a *designed* exact tie between identical rows; the generator
  refuses to emit anything else (research D7).

Run via the pinned virtualenv:

    scripts/setup-reference-venv.sh 004
    reference/.venv-004/bin/python reference/gen_004_fixtures.py --seed 4
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys
from pathlib import Path

# --------------------------------------------------------------------------------------------
# Interpreter guard (the 001 pattern): under the wrong interpreter `import torch` fails in a way
# that looks like a broken machine, and under a stray torch the script silently mints a DIFFERENT
# golden -- much worse, because nothing complains.
# --------------------------------------------------------------------------------------------
_REQUIRED_PY = (3, 12)
if sys.version_info[:2] != _REQUIRED_PY or sys.prefix == sys.base_prefix:
    sys.exit(
        f"gen_004_fixtures.py requires the pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh 004\n"
        "  reference/.venv-004/bin/python reference/gen_004_fixtures.py --seed 4"
    )

# Pin every thread pool BEFORE importing torch (gen_001_fixtures.py does the same). Without this
# the golden embedding's low-order bits vary with the host core count.
for _var in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "OPENBLAS_NUM_THREADS", "NUMEXPR_NUM_THREADS"):
    os.environ[_var] = "1"
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

import numpy as np  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
MODEL_MANIFEST = REPO_ROOT / "reference/models/manifest.json"

# --------------------------------------------------------------------------------------------
# Pinned model and the fingerprint (research D4, D6). The fingerprint is emitted so the Rust
# constant can be asserted equal to it -- the two are built from the same fields.
# --------------------------------------------------------------------------------------------
EMBEDDING_DIM = 384
MAX_SEQ_LEN = 256
COSINE_MIN = 0.9999
MAX_ABS_DIFF = 1e-3
UNIT_NORM_ABS = 1e-5
ENGINE = "candle-0.9.2"

SCORE_ABS_TOL = 1e-6
TIE_MARGIN = 1e-6  # > 8x the f32 ulp at |score| <= 1, so f32 rounding cannot reorder a pair


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


def load_pins() -> dict:
    return json.loads(MODEL_MANIFEST.read_text())


def verify_model_dir(model_dir: Path, pins: dict) -> None:
    """Refuse to mint a golden from anything but the pinned bytes (FR-003 on the Python side)."""
    for entry in pins["files"]:
        path = model_dir / entry["name"]
        if not path.is_file():
            sys.exit(f"missing {path}; run scripts/fetch-model.sh")
        size = path.stat().st_size
        if size != entry["bytes"]:
            sys.exit(f"{path}: {size} bytes, expected {entry['bytes']}")
        digest = sha256_file(path)
        if digest != entry["sha256"]:
            sys.exit(f"{path}: sha256 {digest}, expected {entry['sha256']}")
    print(f"  model verified: {pins['repository']} @ {pins['revision']}")


def fingerprint(pins: dict) -> str:
    weights = next(f for f in pins["files"] if f["name"] == "model.safetensors")
    return (
        f"{pins['repository']}@{pins['revision']}"
        f";weights=sha256:{weights['sha256']}"
        f";dim={pins['dim']};pool=mean-mask;norm=l2;max_tokens={pins['max_tokens']}"
        f";dtype=f32;prefix=none;engine={ENGINE}"
    )


# --------------------------------------------------------------------------------------------
# Embedding oracle (T008). Tokenization and pooling are Feature 001's expressions, copied.
# --------------------------------------------------------------------------------------------

LONG_TEXT = (
    "The retrieval engine indexes every passage of the corpus, scores each one against the "
    "query with a lexical model and a dense model, fuses the two rankings, and returns the "
    "best candidates to the reader. "
) * 25  # ~1,000 words; far past 256 tokens

CASES: list[tuple[str, str]] = [
    ("short", "What is the capital of France?"),
    ("long_over_256", LONG_TEXT),
    ("empty", ""),
    ("whitespace_only", "   \t \n  "),
    ("oov_unicode", "Émile 🚀 東京 naïve façade ☃ Zürich — “quotes” ‘apostrophes’ 日本語のテキスト"),
    ("punctuation_only", "!!! ??? ... --- ;;; ::: ((( )))"),
    (
        "beir_like_title_text",
        "Microstructure-sensitive design of a composite panel "
        "This paper presents a design method for laminated composite panels whose stiffness "
        "and strength are tailored through the fibre orientation of each ply.",
    ),
    ("duplicate_of_short", "What is the capital of France?"),
    ("sentence_medicine", "Patients treated with the drug showed a modest reduction in blood pressure."),
    ("sentence_finance", "How do I report capital gains from selling shares held in a taxable account?"),
    ("sentence_science", "Mitochondria are the primary site of ATP synthesis in eukaryotic cells."),
    ("sentence_numbers", "In 2019 the company shipped 1,204 units at $37.50 each, a 12% increase."),
    ("sentence_code", "fn search(&self, query: &[f32], k: usize) -> Vec<Hit>"),
]


def tokenize(tok, text: str) -> dict:
    enc = tok.encode(text)
    assert len(enc.ids) == MAX_SEQ_LEN, f"expected {MAX_SEQ_LEN} ids, got {len(enc.ids)}"
    assert len(enc.attention_mask) == MAX_SEQ_LEN
    assert set(enc.type_ids) == {0}
    return {
        "input_ids": list(enc.ids),
        "attention_mask": list(enc.attention_mask),
        "n_real_tokens": int(sum(enc.attention_mask)),
    }


def gen_embeddings(model_dir: Path, pins: dict, seed: int) -> dict:
    import torch
    from tokenizers import Tokenizer
    from transformers import AutoModel

    tok = Tokenizer.from_file(str(model_dir / "tokenizer.json"))
    # tokenizer.json ships truncation.max_length=128 and padding Fixed(128); both sides override
    # to 256 (001 research D6).
    tok.enable_truncation(max_length=MAX_SEQ_LEN)
    tok.enable_padding(length=MAX_SEQ_LEN, pad_id=0, pad_token="[PAD]", direction="right")

    torch.manual_seed(seed)
    torch.set_num_threads(1)
    torch.set_grad_enabled(False)
    model = AutoModel.from_pretrained(str(model_dir), dtype=torch.float32)
    model.eval()

    cases = []
    by_id: dict[str, list[float]] = {}
    for case_id, text in CASES:
        t = tokenize(tok, text)
        input_ids = torch.tensor([t["input_ids"]], dtype=torch.long)
        attention_mask = torch.tensor([t["attention_mask"]], dtype=torch.long)
        token_type_ids = torch.zeros_like(input_ids)
        out = model(input_ids=input_ids, attention_mask=attention_mask, token_type_ids=token_type_ids)
        hidden = out.last_hidden_state
        mask = attention_mask.unsqueeze(-1).to(hidden.dtype)
        pooled = (hidden * mask).sum(dim=1) / mask.sum(dim=1)
        normed = torch.nn.functional.normalize(pooled, p=2.0, dim=1)
        vector = [float(x) for x in normed[0].tolist()]
        assert len(vector) == EMBEDDING_DIM
        norm = math.sqrt(sum(x * x for x in vector))
        assert abs(norm - 1.0) < UNIT_NORM_ABS, f"{case_id}: norm {norm}"
        by_id[case_id] = vector
        cases.append({"id": case_id, "text": text, **t, "vector": vector})

    # The edge cases the spec names must be what they claim to be.
    n_real = {c["id"]: c["n_real_tokens"] for c in cases}
    assert n_real["long_over_256"] == MAX_SEQ_LEN, n_real["long_over_256"]
    assert n_real["empty"] == 2, n_real["empty"]  # [CLS] [SEP]
    assert n_real["whitespace_only"] == 2, n_real["whitespace_only"]
    assert by_id["duplicate_of_short"] == by_id["short"]
    assert len(cases) >= 12

    weights = next(f for f in pins["files"] if f["name"] == "model.safetensors")
    return {
        "schema_version": 1,
        "model": {"repository": pins["repository"], "revision": pins["revision"], "weights_sha256": weights["sha256"]},
        "fingerprint": fingerprint(pins),
        "max_sequence_length": MAX_SEQ_LEN,
        "pooling": "mean (attention-mask weighted)",
        "normalize": "l2",
        "dim": EMBEDDING_DIM,
        "torch_version": torch.__version__,
        "tolerance": {"cosine_min": COSINE_MIN, "max_abs_diff": MAX_ABS_DIFF, "unit_norm_abs": UNIT_NORM_ABS},
        "cases": cases,
    }


# --------------------------------------------------------------------------------------------
# Exact-search oracle (T009, T010) -- research D7.
# --------------------------------------------------------------------------------------------


def f32_list(v: np.ndarray) -> list[float]:
    """Serialise float32 values so that parsing the decimal back as f32 is exact."""
    return [float(np.float32(x)) for x in v]


def fdot(a: np.ndarray, b: np.ndarray) -> float:
    """Correctly rounded float64 dot product of two float32 vectors: every product is exact in
    float64 (24 + 24 < 53 bits) and ``math.fsum`` rounds the exact sum once. Order-independent,
    so identical rows give identical scores -- which a BLAS gemv does NOT guarantee (its kernels
    accumulate rows in different orders), and which the designed exact ties depend on."""
    return math.fsum(float(x) * float(y) for x, y in zip(a.tolist(), b.tolist()))


def score(metric: str, q: np.ndarray, rows: np.ndarray) -> np.ndarray:
    """float64 scores of every row for q; inputs are float32 arrays."""
    out = np.empty(len(rows), dtype=np.float64)
    if metric == "cosine":
        qn = math.sqrt(fdot(q, q))
        for j, r in enumerate(rows):
            out[j] = fdot(r, q) / (math.sqrt(fdot(r, r)) * qn)
    elif metric == "dot":
        for j, r in enumerate(rows):
            out[j] = fdot(r, q)
    elif metric == "euclidean":
        for j, r in enumerate(rows):
            d = (r.astype(np.float64) - q.astype(np.float64))
            out[j] = -math.sqrt(math.fsum(x * x for x in d.tolist()))
    else:
        raise ValueError(metric)
    return out


def ranked(scores: np.ndarray, ids: list[int], allowed: set[int] | None) -> list[tuple[int, float]]:
    pairs = [(int(i), float(scores[j])) for j, i in enumerate(ids) if allowed is None or i in allowed]
    pairs.sort(key=lambda p: (-p[1], p[0]))
    return pairs


class Ambiguous(Exception):
    """A near-tie or an undesigned exact tie: the oracle would not be unambiguous after f32 rounding."""


def check_unambiguous(pairs: list[tuple[int, float]], rows_by_id: dict[int, np.ndarray], label: str) -> None:
    """Every consecutive pair in a ranked prefix is either an exact tie between identical rows or
    separated by more than TIE_MARGIN -- so f32 rounding on the Rust side cannot reorder it."""
    for (ia, sa), (ib, sb) in zip(pairs, pairs[1:]):
        if sa == sb:
            if not np.array_equal(rows_by_id[ia], rows_by_id[ib]):
                raise Ambiguous(f"{label}: exact tie between non-identical rows {ia} and {ib}")
        elif abs(sa - sb) < TIE_MARGIN:
            raise Ambiguous(f"{label}: near-tie {sa - sb:.2e} between rows {ia} and {ib}")


def make_rows(rng: np.random.Generator, n: int, dim: int) -> np.ndarray:
    return rng.standard_normal((n, dim)).astype(np.float32)


def plant_tie(metric: str, q: np.ndarray, ids: list[int], rows: np.ndarray, reserved: set[int], prefer_higher: bool) -> tuple[int, int]:
    """Copy the rank-5 row of `q` (or the first unreserved row below it) onto another row so the
    two tie exactly; prefer the copy at a higher (or lower) id so both tie-break directions occur
    across the sets. Returns (src, dst); the caller records the rank the pair lands at."""
    order = [i for i, _ in ranked(score(metric, q, rows), ids, None)]
    src = next(i for i in order[4:] if i not in reserved)  # rank 5 or the first free rank below
    candidates = [i for i in order[8:] if i not in reserved and i != src]
    side = [i for i in candidates if (i > src) == prefer_higher] or candidates
    if not side:
        raise Ambiguous("no row available for the rank-5 tie")
    dst = side[-1] if prefer_higher else side[0]
    rows[dst] = rows[src]
    return src, dst


def build_set(rng: np.random.Generator, set_id: str, dim: int, metric: str, n: int, n_queries: int, ks: list[int], allowed_kinds: list[str], with_ties: bool) -> dict:
    rows = make_rows(rng, n, dim)
    ids = list(range(n))
    queries = [rng.standard_normal(dim).astype(np.float32) for _ in range(n_queries)]
    ks = list(ks)
    designed: list[dict] = []

    if with_ties:
        # Designed duplicates: a triple of identical rows, two random pairs, and for queries 0
        # and 1 a pair that ties exactly around rank 5 (copy at a higher id for q0, lower for q1,
        # when possible). The rank each pair actually lands at is recorded and added to `ks`, so
        # a case exists whose k-th rank is a tie (spec FR-011).
        rows[n - 3] = rows[n - 2] = rows[n - 1]
        rows[10] = rows[20]
        rows[11] = rows[21]
        reserved = {10, 11, 20, 21, n - 3, n - 2, n - 1}
        pairs = []
        for qi, prefer_higher in ((0, True), (1, False)):
            src, dst = plant_tie(metric, queries[qi], ids, rows, reserved, prefer_higher)
            reserved |= {src, dst}
            pairs.append((qi, src, dst))
        for qi, src, dst in pairs:
            order = [i for i, _ in ranked(score(metric, queries[qi], rows), ids, None)]
            pos = min(order.index(src), order.index(dst))
            if order[pos + 1] not in (src, dst):
                raise Ambiguous(f"{set_id} q{qi}: designed pair no longer adjacent")
            k = pos + 1  # the first member sits at rank k; the second is cut off at k
            designed.append({"query": f"q{qi}", "k": k, "ids": sorted((src, dst)), "winner": min(src, dst)})
            if k not in ks:
                ks.append(k)
        ks.sort()

    rows_by_id = {i: rows[i] for i in ids}
    subset = sorted(int(i) for i in rng.choice(n, size=max(1, n // 2), replace=False))
    unseen = [n + 100, n + 101]
    allowed_variants = {
        "none": None,
        "empty": [],
        "subset": subset,
        "subset_unseen": subset + unseen,
    }
    query_entries = []
    for qi, q in enumerate(queries):
        scores = score(metric, q, rows)
        cases = []
        for k in ks:
            for kind in allowed_kinds:
                allowed = allowed_variants[kind]
                aset = None if allowed is None else set(allowed)
                if k == 0 or aset == set():
                    exp = []
                else:
                    full = ranked(scores, ids, aset)
                    check_unambiguous(full[: k + 1], rows_by_id, f"{set_id} q{qi} k={k} allowed={kind}")
                    exp = full[:k]
                cases.append({
                    "k": k,
                    "allowed": allowed,
                    "expected": [{"id": i, "score": s} for i, s in exp],
                })
        query_entries.append({"id": f"q{qi}", "vector": f32_list(q), "cases": cases})

    return {
        "id": set_id,
        "dim": dim,
        "metric": metric,
        "designed_ties": designed,
        "rows": [{"id": i, "vector": f32_list(rows[i])} for i in ids],
        "queries": query_entries,
    }


def gen_search(seed: int) -> dict:
    """Retry seeds until every set is unambiguous (research D7); say so when it happens."""
    for attempt in range(50):
        rng = np.random.default_rng(seed + attempt)
        try:
            n8 = 50
            sets = [
                build_set(rng, "dim8_cosine_ties", 8, "cosine", n8, 3, [0, 1, 5, 10, n8, n8 + 5], ["none", "empty", "subset", "subset_unseen"], True),
                build_set(rng, "dim8_dot", 8, "dot", n8, 3, [0, 1, 5, 10, n8, n8 + 5], ["none", "empty", "subset", "subset_unseen"], True),
                build_set(rng, "dim8_euclidean", 8, "euclidean", n8, 3, [0, 1, 5, 10, n8, n8 + 5], ["none", "empty", "subset", "subset_unseen"], True),
                build_set(rng, "dim384_cosine", EMBEDDING_DIM, "cosine", 120, 3, [1, 10, 100], ["none", "subset"], True),
            ]
        except Ambiguous as e:
            print(f"  reroll: seed {seed + attempt} rejected -- {e}")
            continue
        return {
            "schema_version": 1,
            "seed": seed + attempt,
            "score_abs_tol": SCORE_ABS_TOL,
            "tie_margin": TIE_MARGIN,
            "sets": sets,
        }
    sys.exit("gen_search: no unambiguous seed found in 50 attempts")


def gen_mutations(seed: int) -> dict:
    """A scripted add/replace/delete/commit/reopen sequence with expected state after each commit."""
    dim, metric = 8, "dot"
    for attempt in range(50):
        rng = np.random.default_rng(seed + 1000 + attempt)
        live: dict[int, np.ndarray] = {}
        pending: dict[int, np.ndarray | None] = {}
        steps: list[dict] = []
        q = rng.standard_normal(dim).astype(np.float32)

        def commit() -> None:
            for i, v in pending.items():
                if v is None:
                    live.pop(i, None)
                else:
                    live[i] = v
            pending.clear()
            steps.append({"op": "commit"})

        def expect(k: int) -> None:
            ids = sorted(live)
            rows = np.stack([live[i] for i in ids]) if ids else np.zeros((0, dim), np.float32)
            full = ranked(score(metric, q, rows), ids, None) if ids else []
            check_unambiguous(full[: k + 1], {i: live[i] for i in ids}, f"mutations step {len(steps)}")
            exp = full[:k]
            steps.append({"op": "expect", "len": len(ids), "query": f32_list(q), "k": k,
                          "results": [{"id": i, "score": s} for i, s in exp]})

        def add(i: int, v: np.ndarray) -> None:
            pending[i] = v
            steps.append({"op": "add", "id": i, "vector": f32_list(v)})

        try:
            for i in range(10):
                add(i, rng.standard_normal(dim).astype(np.float32))
            commit(); expect(10)
            # replace: invisible until commit, then id appears once with the new vector
            add(3, rng.standard_normal(dim).astype(np.float32))
            expect(10)  # pending not visible: same as before
            commit(); expect(10)
            # delete a known and an unknown id
            pending[5] = None
            steps.append({"op": "delete", "ids": [5, 99]})
            commit(); expect(10)
            # add, then reopen without commit: the add is gone
            add(20, rng.standard_normal(dim).astype(np.float32))
            pending.clear()
            steps.append({"op": "reopen"})
            expect(10)
            # a designed tie at the last rank when k == len: id 21 duplicates row 7
            add(21, live[7].copy())
            commit(); expect(len(live))
        except Ambiguous as e:
            print(f"  reroll (mutations): seed {seed + 1000 + attempt} rejected -- {e}")
            continue
        return {"schema_version": 1, "seed": seed + 1000 + attempt, "dim": dim, "metric": metric,
                "fingerprint": "test-fp", "score_abs_tol": SCORE_ABS_TOL, "steps": steps}
    sys.exit("gen_mutations: no unambiguous seed found")


# --------------------------------------------------------------------------------------------
# --verify-embed (T011): re-embed Rust-exported vectors with torch and report the worst case.
# --------------------------------------------------------------------------------------------


def verify_embed(model_dir: Path, pins: dict, path: Path) -> int:
    import torch
    from tokenizers import Tokenizer
    from transformers import AutoModel

    tok = Tokenizer.from_file(str(model_dir / "tokenizer.json"))
    tok.enable_truncation(max_length=MAX_SEQ_LEN)
    tok.enable_padding(length=MAX_SEQ_LEN, pad_id=0, pad_token="[PAD]", direction="right")
    torch.set_num_threads(1)
    torch.set_grad_enabled(False)
    model = AutoModel.from_pretrained(str(model_dir), dtype=torch.float32)
    model.eval()

    worst_cos, worst_abs, n, bad = 1.0, 0.0, 0, 0
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        rec = json.loads(line)
        t = tokenize(tok, rec["text"])
        input_ids = torch.tensor([t["input_ids"]], dtype=torch.long)
        attention_mask = torch.tensor([t["attention_mask"]], dtype=torch.long)
        out = model(input_ids=input_ids, attention_mask=attention_mask, token_type_ids=torch.zeros_like(input_ids))
        mask = attention_mask.unsqueeze(-1).to(out.last_hidden_state.dtype)
        pooled = (out.last_hidden_state * mask).sum(dim=1) / mask.sum(dim=1)
        ref = torch.nn.functional.normalize(pooled, p=2.0, dim=1)[0].numpy().astype(np.float64)
        got = np.asarray(rec["vector"], dtype=np.float64)
        cos = float(got @ ref / (np.linalg.norm(got) * np.linalg.norm(ref)))
        mad = float(np.abs(got - ref).max())
        worst_cos, worst_abs, n = min(worst_cos, cos), max(worst_abs, mad), n + 1
        if cos < COSINE_MIN or mad > MAX_ABS_DIFF:
            bad += 1
            print(f"  FAIL doc {rec['doc_id']}: cosine {cos:.6f} max_abs {mad:.2e}")
    print(f"verify-embed: {n} vectors, worst cosine {worst_cos:.6f} (min {COSINE_MIN}), worst max_abs {worst_abs:.2e} (max {MAX_ABS_DIFF}), {bad} outside tolerance")
    return 1 if bad or n == 0 else 0


# --------------------------------------------------------------------------------------------


def write_manifest(out: Path) -> None:
    files = sorted(p.name for p in out.iterdir() if p.suffix == ".json" and p.name != "manifest.json")
    write_json(out / "manifest.json", {
        "schema_version": 1,
        "files": {name: sha256_file(out / name) for name in files},
        "generator_sha256": sha256_file(Path(__file__)),
        "python_version": ".".join(map(str, sys.version_info[:3])),
    })


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=int, default=4)
    ap.add_argument("--out", type=Path, default=REPO_ROOT / "reference/fixtures/004")
    ap.add_argument("--model-dir", type=Path, default=REPO_ROOT / "reference/models/all-MiniLM-L6-v2")
    ap.add_argument("--refresh-manifest", action="store_true", help="only rewrite manifest.json")
    ap.add_argument("--verify-embed", type=Path, metavar="VECTORS_JSONL",
                    help="re-embed {doc_id,text,vector} lines with torch and report the worst case")
    args = ap.parse_args()

    pins = load_pins()
    if args.refresh_manifest:
        write_manifest(args.out)
        return 0
    verify_model_dir(args.model_dir, pins)
    if args.verify_embed:
        return verify_embed(args.model_dir, pins, args.verify_embed)

    print("embeddings.json")
    write_json(args.out / "embeddings.json", gen_embeddings(args.model_dir, pins, args.seed))
    print("search.json")
    write_json(args.out / "search.json", gen_search(args.seed))
    print("mutations.json")
    write_json(args.out / "mutations.json", gen_mutations(args.seed))
    write_manifest(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
