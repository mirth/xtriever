#!/usr/bin/env python3
"""Generate the golden fixtures for Feature 006 (the re-rank stage).

Two oracles (Principle II), written by the same script:

* **Reference scores** -- ``rerank.json``: the pinned ``transformers`` sequence-classification
  pipeline over ``cross-encoder/ms-marco-MiniLM-L-6-v2`` (research D1/D4): one query--passage pair
  per forward pass, ``truncation=True, max_length=512`` (``longest_first``), no padding, the raw
  logit. Each pair also carries its token ids so the Rust side can prove tokenization parity
  before comparing a single score. Compared at max-abs <= 1e-3 and exact per-query order; the
  generator refuses any query whose adjacent score gap is below ``MIN_GAP`` (10x the tolerance),
  so "exact order" is a fair test.
* **Ordering rule** -- ``pipeline_order.json``: research D8 (scored first by ``(-score, id)``,
  then the unscored in fused order, cut at ``k``) computed independently in Python.

Run via the 004 virtualenv (same torch/transformers/tokenizers pins -- the oracle is the same
stack scoring a different model):

    scripts/setup-reference-venv.sh 004
    reference/.venv-004/bin/python reference/gen_006_fixtures.py --out reference/fixtures/006/
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

# --------------------------------------------------------------------------------------------
# Interpreter guard (the 001/004 pattern): under a stray torch the script silently mints a
# DIFFERENT golden, which is much worse than failing.
# --------------------------------------------------------------------------------------------
_REQUIRED_PY = (3, 12)
if sys.version_info[:2] != _REQUIRED_PY or sys.prefix == sys.base_prefix:
    sys.exit(
        f"gen_006_fixtures.py requires the pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh 004\n"
        "  reference/.venv-004/bin/python reference/gen_006_fixtures.py --out reference/fixtures/006/"
    )

# Pin every thread pool BEFORE importing torch; the golden's low-order bits must not depend on
# the host core count.
for _var in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "OPENBLAS_NUM_THREADS", "NUMEXPR_NUM_THREADS"):
    os.environ[_var] = "1"
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

REPO_ROOT = Path(__file__).resolve().parent.parent
MODEL_MANIFEST = REPO_ROOT / "reference/models/manifest-rerank.json"
DEFAULT_MODEL_DIR = REPO_ROOT / "reference/models/ms-marco-MiniLM-L-6-v2"

MAX_TOKENS = 512
TOLERANCE_ABS = 1e-3
MIN_GAP = 10 * TOLERANCE_ABS
NEAR_TIE_MAX = 1.0
ENGINE = "candle-0.9.2"


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
            sys.exit(f"missing {path}; run scripts/fetch-model.sh --manifest {MODEL_MANIFEST}")
        size = path.stat().st_size
        if size != entry["bytes"]:
            sys.exit(f"{path}: {size} bytes, expected {entry['bytes']}")
        digest = sha256_file(path)
        if digest != entry["sha256"]:
            sys.exit(f"{path}: sha256 {digest}, expected {entry['sha256']}")
    print(f"  model verified: {pins['repository']} @ {pins['revision']}")


def model_id(pins: dict) -> str:
    """The Rust `MODEL_ID` constant, assembled from the same fields (data-model)."""
    weights = next(f for f in pins["files"] if f["name"] == "model.safetensors")
    return (
        f"{pins['repository']}@{pins['revision']}"
        f";weights=sha256:{weights['sha256']}"
        f";max_tokens={pins['max_tokens']};trunc=longest_first;head=cls-pooler-tanh-linear"
        f";act=identity;dtype=f32;engine={ENGINE}"
    )


# --------------------------------------------------------------------------------------------
# Reference scorer (T008): the HF sequence-classification pipeline, one pair per forward.
# --------------------------------------------------------------------------------------------


class Reference:
    def __init__(self, model_dir: Path) -> None:
        import torch
        from transformers import AutoModelForSequenceClassification, AutoTokenizer

        self.torch = torch
        self.tokenizer = AutoTokenizer.from_pretrained(model_dir)
        if not self.tokenizer.is_fast:
            sys.exit("the reference tokenizer must be the fast (tokenizers-backed) one")
        self.model = AutoModelForSequenceClassification.from_pretrained(model_dir).eval()
        if self.model.config.num_labels != 1:
            sys.exit(f"expected a single-label head, got {self.model.config.num_labels}")
        self._checked_head = False

    def encode(self, query: str, passage: str) -> dict:
        # `truncation=True` is `longest_first`; an empty passage is falsy and transformers then
        # encodes the query alone (research D4) -- the Rust side mirrors exactly this call.
        return self.tokenizer(query, passage, truncation=True, max_length=MAX_TOKENS, return_tensors="pt")

    def score(self, query: str, passage: str) -> dict:
        enc = self.encode(query, passage)
        with self.torch.no_grad():
            logit = self.model(**enc).logits[0, 0].item()
            if not self._checked_head:
                # Guard the research D1 reading of the head: CLS -> pooler -> tanh -> classifier.
                hidden = self.model.bert(**enc).last_hidden_state
                pooled = self.torch.tanh(self.model.bert.pooler.dense(hidden[:, 0]))
                manual = self.model.classifier(pooled)[0, 0].item()
                if manual != logit:
                    sys.exit(f"head mismatch: manual {manual!r} vs pipeline {logit!r}")
                self._checked_head = True
        ids = enc["input_ids"][0].tolist()
        return {
            "input_ids": ids,
            "token_type_ids": enc["token_type_ids"][0].tolist(),
            "truncated": len(ids) == MAX_TOKENS,
            "score": float(logit),
        }


# --------------------------------------------------------------------------------------------
# The golden set (T009): hand-written queries with an on-topic passage, distractors and an
# off-topic one, plus the named edge cases. Texts are edited by hand until every adjacent gap
# clears MIN_GAP; the generator refuses to emit otherwise.
# --------------------------------------------------------------------------------------------

LONG_PASSAGE = (
    "The retrieval engine indexes every passage of the corpus, scores each one against the "
    "query with a lexical model and a dense model, fuses the two rankings, and hands the best "
    "candidates to a cross-encoder that reads the query and the passage together. "
) * 20  # ~640 words; far past 512 tokens together with any query

QUERIES: list[dict] = [
    {
        "name": "berlin-population",
        "query": "How many people live in Berlin?",
        "passages": [
            "Berlin has a population of 3,520,031 registered inhabitants in an area of 891.82 square kilometers.",
            "Berlin is the capital of Germany and is known for its museums, theatres and nightlife.",
            "The population of Hamburg is about 1.9 million, making it the second-largest German city.",
            "New York City is famous for the Metropolitan Museum of Art.",
        ],
    },
    {
        "name": "python-creator",
        "query": "Who created the Python programming language?",
        "passages": [
            "Python was created by Guido van Rossum and first released in 1991.",
            "Python is a high-level programming language known for its readable syntax.",
            "The Python is a family of nonvenomous snakes found in Africa, Asia and Australia.",
            "Rust was originally designed by Graydon Hoare at Mozilla Research.",
        ],
    },
    {
        "name": "boiling-point",
        "query": "At what temperature does water boil at sea level?",
        "passages": [
            "At sea level water boils at 100 degrees Celsius, or 212 degrees Fahrenheit.",
            "The boiling point of a liquid decreases as the atmospheric pressure drops with altitude.",
            "Water freezes at 0 degrees Celsius under standard atmospheric pressure.",
            "Sea level is the average height of the ocean surface between high and low tide.",
            "The recipe calls for two cups of flour and a teaspoon of salt.",
        ],
    },
    {
        "name": "over-length",
        "query": "What does a cross-encoder read?",
        "passages": [
            LONG_PASSAGE,
            "A cross-encoder reads the query and the passage together and outputs one relevance score.",
            "A bi-encoder embeds the query and the passage separately and compares the vectors.",
        ],
    },
    {
        "name": "empty-passage",
        "query": "What is the capital of France?",
        "passages": [
            "Paris is the capital and most populous city of France.",
            "",
            "Lyon is a city in the Auvergne-Rhone-Alpes region of France.",
        ],
    },
    {
        "name": "empty-query",
        "query": "",
        "passages": [
            "The quick brown fox jumps over the lazy dog.",
            "Interest rates were left unchanged by the central bank on Thursday.",
            "Photosynthesis converts light energy into chemical energy in plants.",
        ],
    },
    {
        "name": "both-empty",
        "query": "",
        "passages": [
            "",
            "A sentence that is not empty.",
        ],
    },
    {
        "name": "near-tie",
        "query": "When was the Eiffel Tower completed?",
        "passages": [
            "The Eiffel Tower was completed in 1889 for the World's Fair in Paris.",
            "The Eiffel Tower was completed in 1889 for the World's Fair held in Paris.",
            "The Eiffel Tower is 330 metres tall and was the tallest structure in the world until 1930.",
            "The Statue of Liberty was dedicated in 1886 in New York Harbor.",
        ],
    },
    {
        "name": "mortgage-rate",
        "query": "How is a fixed-rate mortgage different from an adjustable-rate mortgage?",
        "passages": [
            "A fixed-rate mortgage keeps the same interest rate for the whole term, while an adjustable-rate mortgage changes its rate periodically after an initial period.",
            "Mortgage lenders require proof of income and a credit check before approving a loan.",
            "An index fund tracks a market index such as the S&P 500 at low cost.",
            "The term of a mortgage is commonly 15 or 30 years in the United States.",
        ],
    },
    {
        "name": "vitamin-d",
        "query": "Which foods are rich in vitamin D?",
        "passages": [
            "Fatty fish such as salmon and mackerel, egg yolks and fortified milk are rich sources of vitamin D.",
            "Vitamin D is produced in the skin when it is exposed to sunlight.",
            "Oranges and other citrus fruits are well-known sources of vitamin C.",
            "A balanced diet includes vegetables, whole grains and lean protein.",
            "The museum opens at nine in the morning on weekdays.",
        ],
    },
]


def gen_rerank(ref: Reference, pins: dict) -> dict:
    queries = []
    for case in QUERIES:
        scored = [dict(text=p, **ref.score(case["query"], p)) for p in case["passages"]]
        order = sorted(range(len(scored)), key=lambda i: (-scored[i]["score"], i))
        sorted_scores = [scored[i]["score"] for i in order]
        gaps = [a - b for a, b in zip(sorted_scores, sorted_scores[1:])]
        if gaps and min(gaps) < MIN_GAP:
            sys.exit(
                f"query {case['name']!r}: adjacent score gap {min(gaps):.6f} below MIN_GAP "
                f"{MIN_GAP}; scores {sorted_scores} -- edit the passages so exact order is a fair test"
            )
        if case["name"] == "near-tie":
            tie_gap = abs(scored[0]["score"] - scored[1]["score"])
            if not (MIN_GAP <= tie_gap <= NEAR_TIE_MAX):
                sys.exit(f"near-tie gap {tie_gap:.6f} outside [{MIN_GAP}, {NEAR_TIE_MAX}]")
        if case["name"] == "over-length" and not scored[0]["truncated"]:
            sys.exit("over-length passage was not truncated to 512")
        if case["name"] == "empty-passage":
            ids = scored[1]["input_ids"]
            if ids.count(102) != 1:
                sys.exit(f"empty passage must encode as a single sequence, got {ids}")
        print(f"  {case['name']:18s} {[round(s, 4) for s in sorted_scores]}")
        queries.append({
            "name": case["name"],
            "query": case["query"],
            "passages": scored,
            "order": order,
        })
    return {
        "model_id": model_id(pins),
        "max_tokens": MAX_TOKENS,
        "tolerance_abs": TOLERANCE_ABS,
        "min_gap": MIN_GAP,
        "queries": queries,
    }


# --------------------------------------------------------------------------------------------
# Ordering oracle (T010): research D8, independently.
# --------------------------------------------------------------------------------------------


def order_reranked(fused: list[int], scores: list[float | None], d: int, k: int) -> list[list]:
    """Scored (i < d and scores[i] is not None) first by (-score, id); then the rest in fused
    order; cut at k. Returns [[id, score-or-None], ...]."""
    scored = [(fused[i], scores[i]) for i in range(min(d, len(fused))) if scores[i] is not None]
    scored_ids = {i for i, _ in scored}
    scored.sort(key=lambda t: (-t[1], t[0]))
    rest = [(i, None) for i in fused if i not in scored_ids]
    return [[i, s] for i, s in (scored + rest)[:k]]


ORDER_CASES: list[dict] = [
    dict(name="full", fused=[7, 3, 9, 1, 4], scores=[0.5, 1.5, 2.0, -1.0, 0.0], d=5, k=5),
    dict(name="partial-m-lt-d", fused=[7, 3, 9, 1, 4], scores=[0.5, None, 2.0, None], d=4, k=5),
    dict(name="none-scored", fused=[7, 3, 9, 1, 4], scores=[None, None, None], d=3, k=5),
    dict(name="d-gt-k", fused=[7, 3, 9, 1, 4, 8, 2], scores=[0.1, 0.2, 0.3, 0.4, 0.5, 9.0], d=6, k=3),
    dict(name="d-lt-k", fused=[7, 3, 9, 1, 4], scores=[-1.0, 1.0], d=2, k=5),
    dict(name="d-ge-len", fused=[7, 3, 9], scores=[1.0, 2.0, 3.0], d=10, k=10),
    dict(name="d-zero", fused=[7, 3, 9, 1], scores=[], d=0, k=4),
    dict(name="ties-by-id", fused=[9, 7, 3, 1], scores=[1.0, 1.0, 1.0, 0.5], d=4, k=4),
    dict(name="k-lt-scored", fused=[7, 3, 9, 1], scores=[1.0, 3.0, 2.0, 4.0], d=4, k=2),
    dict(name="single", fused=[5], scores=[0.25], d=1, k=1),
]


def gen_pipeline_order() -> dict:
    cases = []
    for c in ORDER_CASES:
        expected = order_reranked(c["fused"], c["scores"], c["d"], c["k"])
        cases.append({**c, "expected": expected})
    return {"cases": cases}


# --------------------------------------------------------------------------------------------
# Verification against real runs (T011).
# --------------------------------------------------------------------------------------------


def verify_rerank(explain_path: Path) -> int:
    """Each line: {"query_id", "fused": [ext ids], "rerank": [[rank, ext id, score], ...],
    "hits": [ext ids]}. The scored prefix of `hits` must be the `rerank` ids ordered by
    (-score) with ties compared as blocks; the suffix must be `fused` minus the scored ids."""
    checked = bad = 0
    with explain_path.open() as fh:
        for line in fh:
            if not line.strip():
                continue
            rec = json.loads(line)
            checked += 1
            rerank = sorted(rec["rerank"], key=lambda t: (-t[2], t[0]))
            scored_ids = [t[1] for t in rerank]
            hits = rec["hits"]
            prefix, suffix = hits[: len(scored_ids)], hits[len(scored_ids):]
            ok = True
            # Compare tie blocks as sets: the pipeline breaks ties by internal id.
            i = 0
            while i < len(rerank) and ok:
                j = i
                while j < len(rerank) and rerank[j][2] == rerank[i][2]:
                    j += 1
                if set(prefix[i:j]) != set(scored_ids[i:j]):
                    ok = False
                i = j
            expected_suffix = [x for x in rec["fused"] if x not in set(scored_ids)]
            expected_suffix = expected_suffix[: len(suffix)]
            if suffix != expected_suffix:
                ok = False
            if not ok:
                bad += 1
                if bad <= 5:
                    print(f"  MISMATCH query {rec['query_id']}: hits {hits[:12]} rerank {scored_ids[:12]}")
    print(f"verify-rerank: {checked} queries checked, {bad} disagree")
    return 1 if bad else 0


def verify_scores(model_dir: Path, sample_path: Path) -> int:
    """Each line: {"query", "passage", "score"}; re-score with torch and report the worst case."""
    ref = Reference(model_dir)
    worst = 0.0
    n = 0
    with sample_path.open() as fh:
        for line in fh:
            if not line.strip():
                continue
            rec = json.loads(line)
            n += 1
            diff = abs(ref.score(rec["query"], rec["passage"])["score"] - rec["score"])
            worst = max(worst, diff)
    print(f"verify-scores: {n} pairs, max abs diff {worst:.3e} (tolerance {TOLERANCE_ABS})")
    return 0 if worst <= TOLERANCE_ABS else 1


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
    ap.add_argument("--out", type=Path, default=REPO_ROOT / "reference/fixtures/006")
    ap.add_argument("--model-dir", type=Path, default=DEFAULT_MODEL_DIR)
    ap.add_argument("--refresh-manifest", action="store_true", help="only rewrite manifest.json")
    ap.add_argument("--verify-rerank", type=Path, metavar="EXPLAIN_JSONL",
                    help="check the ordering rule on an exported explain file")
    ap.add_argument("--verify-scores", type=Path, metavar="SAMPLE_JSONL",
                    help="re-score {query,passage,score} lines with torch and report the worst case")
    args = ap.parse_args()

    pins = load_pins()
    if args.refresh_manifest:
        write_manifest(args.out)
        return 0
    if args.verify_rerank:
        return verify_rerank(args.verify_rerank)
    verify_model_dir(args.model_dir, pins)
    print(f"  model_id: {model_id(pins)}")
    if args.verify_scores:
        return verify_scores(args.model_dir, args.verify_scores)

    ref = Reference(args.model_dir)
    print("rerank.json")
    write_json(args.out / "rerank.json", gen_rerank(ref, pins))
    print("pipeline_order.json")
    write_json(args.out / "pipeline_order.json", gen_pipeline_order())
    write_manifest(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
