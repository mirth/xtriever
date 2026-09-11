#!/usr/bin/env python3
"""Generate the golden fixtures for Feature 001 (iOS build spike).

Principle II requires behaviour with a reference implementation to be verified against golden
fixtures produced by scripts in ``reference/``. This script produces all of them.

Two oracles, deliberately kept separate because one fixture cannot serve both (report.md D-001):

* **BM25 parity** -- ``bm25_reference.json`` is an *independent* Python transcription of tantivy's
  documented BM25. Compared ids-and-order exact, scores within ``score_rel_tol`` (1e-5). Answers
  "is our BM25 the BM25?".
* **Host<->device determinism** -- ``ranking.json`` is minted from a real host tantivy run and
  compared bit-exact. Answers "does iOS produce the same bits as macOS?". This script writes a
  placeholder for it until the Rust side lands (PR 2).

Run via the pinned virtualenv:

    scripts/setup-reference-venv.sh
    reference/.venv-001/bin/python reference/gen_001_fixtures.py --seed 1
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import sys
from bisect import bisect_left
from pathlib import Path

# --------------------------------------------------------------------------------------------
# Interpreter guard. This is the same class of trap as the Homebrew-rustc shadowing in research
# D14, and deserves the same treatment: under 3.14 the failure is `import torch` blowing up in a
# way that looks like a broken machine, and under a stray interpreter with a different torch the
# script silently produces a DIFFERENT golden -- much worse, because nothing complains.
# --------------------------------------------------------------------------------------------
_REQUIRED_PY = (3, 12)
if sys.version_info[:2] != _REQUIRED_PY or sys.prefix == sys.base_prefix:
    sys.exit(
        f"gen_001_fixtures.py requires the pinned venv on Python "
        f"{_REQUIRED_PY[0]}.{_REQUIRED_PY[1]} (running "
        f"{sys.version_info[0]}.{sys.version_info[1]}, "
        f"{'venv' if sys.prefix != sys.base_prefix else 'system interpreter'}).\n"
        "  scripts/setup-reference-venv.sh\n"
        "  reference/.venv-001/bin/python reference/gen_001_fixtures.py --seed 1"
    )

# Pin every thread pool BEFORE importing torch. Without this the golden embedding's low-order
# bits vary with the host core count, and `--check` becomes noise. This is the Python mirror of
# CANDLE_NUM_THREADS=1 on the Rust side (research D15).
for _var in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "OPENBLAS_NUM_THREADS", "NUMEXPR_NUM_THREADS"):
    os.environ[_var] = "1"
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

# --------------------------------------------------------------------------------------------
# Pinned model (research D7). Every value here is asserted, never assumed.
# --------------------------------------------------------------------------------------------
MODEL_REPO = "sentence-transformers/all-MiniLM-L6-v2"
MODEL_REVISION = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
WEIGHTS_BYTES = 90_868_376
WEIGHTS_FILE = "model.safetensors"
MODEL_FILES = (WEIGHTS_FILE, "config.json", "tokenizer.json")
EMBEDDING_DIM = 384
# tokenizer.json bakes in 128; sentence_bert_config.json and the model card say 256. 256 is what
# reproduces reference sentence-transformers behaviour (research D6).
MAX_SEQ_LEN = 256
COSINE_MIN = 0.9999
MAX_ABS_DIFF = 1e-3
SCORE_REL_TOL = 1e-5  # BM25 parity band; stated in spec.md Assumptions

# --------------------------------------------------------------------------------------------
# tantivy 0.26.2 BM25, transcribed. Every constant is cited; none is folklore.
#   src/query/bm25.rs:8-9      K1 = 1.2, B = 0.75
#   src/query/bm25.rs:52-56    idf = ln(1 + (N - df + 0.5) / (df + 0.5))
#   src/query/bm25.rs:168      weight = idf * (1.0 + K1)
#   src/query/bm25.rs:58-59    tf_component = K1 * (1 - B + B * fieldnorm / avg_fieldnorm)
#   src/query/bm25.rs:190-193  tf_factor  = tf / (tf + tf_component)
#   src/query/bm25.rs:111      avg_fieldnorm = total_num_tokens / total_num_docs
#
# The subtlety a naive BM25 gets wrong: only the PER-DOCUMENT length is quantized (to a u8 id via
# FIELD_NORMS_TABLE); the average is computed from raw, unquantized token counts.
# --------------------------------------------------------------------------------------------
K1 = 1.2
B = 0.75

# src/fieldnorm/code.rs:13 -- extracted verbatim, identity for ids 0..=40 then coarsening.
FIELD_NORMS_TABLE: tuple[int, ...] = (
    0, 1, 2, 3, 4, 5, 6, 7,
    8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23,
    24, 25, 26, 27, 28, 29, 30, 31,
    32, 33, 34, 35, 36, 37, 38, 39,
    40, 42, 44, 46, 48, 50, 52, 54,
    56, 60, 64, 68, 72, 76, 80, 84,
    88, 96, 104, 112, 120, 128, 136, 144,
    152, 168, 184, 200, 216, 232, 248, 264,
    280, 312, 344, 376, 408, 440, 472, 504,
    536, 600, 664, 728, 792, 856, 920, 984,
    1048, 1176, 1304, 1432, 1560, 1688, 1816, 1944,
    2072, 2328, 2584, 2840, 3096, 3352, 3608, 3864,
    4120, 4632, 5144, 5656, 6168, 6680, 7192, 7704,
    8216, 9240, 10264, 11288, 12312, 13336, 14360, 15384,
    16408, 18456, 20504, 22552, 24600, 26648, 28696, 30744,
    32792, 36888, 40984, 45080, 49176, 53272, 57368, 61464,
    65560, 73752, 81944, 90136, 98328, 106520, 114712, 122904,
    131096, 147480, 163864, 180248, 196632, 213016, 229400, 245784,
    262168, 294936, 327704, 360472, 393240, 426008, 458776, 491544,
    524312, 589848, 655384, 720920, 786456, 851992, 917528, 983064,
    1048600, 1179672, 1310744, 1441816, 1572888, 1703960, 1835032, 1966104,
    2097176, 2359320, 2621464, 2883608, 3145752, 3407896, 3670040, 3932184,
    4194328, 4718616, 5242904, 5767192, 6291480, 6815768, 7340056, 7864344,
    8388632, 9437208, 10485784, 11534360, 12582936, 13631512, 14680088, 15728664,
    16777240, 18874392, 20971544, 23068696, 25165848, 27263000, 29360152, 31457304,
    33554456, 37748760, 41943064, 46137368, 50331672, 54525976, 58720280, 62914584,
    67108888, 75497496, 83886104, 92274712, 100663320, 109051928, 117440536, 125829144,
    134217752, 150994968, 167772184, 184549400, 201326616, 218103832, 234881048, 251658264,
    268435480, 301989912, 335544344, 369098776, 402653208, 436207640, 469762072, 503316504,
    536870936, 603979800, 671088664, 738197528, 805306392, 872415256, 939524120, 1006632984,
    1073741848, 1207959576, 1342177304, 1476395032, 1610612760, 1744830488, 1879048216, 2013265944,
)
assert len(FIELD_NORMS_TABLE) == 256
assert list(FIELD_NORMS_TABLE) == sorted(FIELD_NORMS_TABLE)


def fieldnorm_to_id(fieldnorm: int) -> int:
    """tantivy src/fieldnorm/code.rs:9 -- ``binary_search(..).unwrap_or_else(|idx| idx - 1)``."""
    idx = bisect_left(FIELD_NORMS_TABLE, fieldnorm)
    if idx < len(FIELD_NORMS_TABLE) and FIELD_NORMS_TABLE[idx] == fieldnorm:
        return idx
    return idx - 1


def id_to_fieldnorm(fieldnorm_id: int) -> int:
    """tantivy src/fieldnorm/code.rs:3."""
    return FIELD_NORMS_TABLE[fieldnorm_id]


def analyze(text: str) -> list[str]:
    """tantivy's ``default`` analyzer, transcribed.

    ``SimpleTokenizer -> RemoveLongFilter::limit(40) -> LowerCaser``
    (src/tokenizer/tokenizer_manager.rs:56-64).

    * SimpleTokenizer splits on ``!char::is_alphanumeric()`` (src/tokenizer/simple_tokenizer.rs:34).
    * RemoveLongFilter keeps a token when ``token.text.len() < 40`` -- that is the **byte** length,
      and it runs **before** LowerCaser (src/tokenizer/remove_long.rs:36).
    """
    tokens: list[str] = []
    current: list[str] = []
    for ch in text:
        if ch.isalnum():
            current.append(ch)
        elif current:
            tokens.append("".join(current))
            current = []
    if current:
        tokens.append("".join(current))
    # RemoveLongFilter (byte length, pre-lowercase), then LowerCaser.
    return [t.lower() for t in tokens if len(t.encode("utf-8")) < 40]


def idf(doc_freq: int, doc_count: int) -> float:
    """tantivy src/query/bm25.rs:52-56."""
    x = ((doc_count - doc_freq) + 0.5) / (doc_freq + 0.5)
    return math.log(1.0 + x)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(f"  wrote {path.relative_to(REPO_ROOT)}")


REPO_ROOT = Path(__file__).resolve().parent.parent


# --------------------------------------------------------------------------------------------
# Fixture generators. One function per task, each independently runnable via --only.
# --------------------------------------------------------------------------------------------

WORD_BANK = (
    "harbour lantern compass meadow granite tunnel orchard beacon quarry thicket "
    "riverbed cinder gantry parapet furrow saltmarsh kiln trellis brackish weir "
    "index shard token vector cursor segment posting merge commit reader writer "
    "query score filter rank fusion embed passage corpus chunk"
).split()

# Planted rare terms give the query something discriminative to match. Without them every
# document looks alike and the top-k scores tie, which would make the parity oracle's
# order-exactness undefendable.
RARE_TERMS = ("zephyrine", "quillon", "vantablack", "obsidian")


def gen_corpus(seed: int) -> dict:
    """T008 -- 1,000 short ASCII documents, one query, one sentence, from a fixed seed.

    The query term is planted deliberately rather than sprinkled randomly. A single-term query over
    documents of similar length produces *identical* BM25 scores -- score depends only on (term
    frequency, quantized field length) -- and a top-k full of ties cannot support an order-exact
    oracle. So the documents that match the query are constructed with pairwise-distinct
    (tf, token count) pairs, which guarantees distinct scores. Everything else is filler that does
    not contain the query term at all.
    """
    rng = random.Random(seed)

    def sentence_of(n_words: int, extra: list[str] | None = None) -> str:
        words = [rng.choice(WORD_BANK) for _ in range(n_words)]
        for term in extra or []:
            words.insert(rng.randrange(len(words) + 1), term)
        return " ".join(words).capitalize() + "."

    query = RARE_TERMS[0]
    documents: list[dict] = []

    # Filler: never contains the query term. The wide word-count range matters -- it puts some
    # documents above the fieldnorm table's identity range (ids 0..=40), which is what exercises
    # the u8 quantization the BM25 transcription has to reproduce.
    for i in range(1000):
        text = " ".join(sentence_of(rng.randint(6, 20)) for _ in range(rng.randint(1, 3)))
        documents.append({"external_id": f"doc-{i:06d}", "text": text})

    # Planted matches: distinct (tf, token count) => distinct scores.
    planted_positions = rng.sample(range(1000), 15)
    for j, pos in enumerate(sorted(planted_positions)):
        tf = 1 + (j % 5)
        filler_words = 8 + 2 * j            # distinct per document, so field lengths differ
        text = sentence_of(filler_words, extra=[query] * tf)
        documents[pos] = {"external_id": f"doc-{pos:06d}", "text": text}

    for doc in documents:
        assert doc["text"].isascii(), "corpus must be ASCII-only (data-model.md FixtureDocument)"

    sentence = "A lantern on the harbour wall marks the quarry road."
    assert sentence.isascii()

    matching = sum(1 for d in documents if query in analyze(d["text"]))
    assert matching >= 1, f"query {query!r} matches no document"

    return {
        "schema_version": 1,
        "seed": seed,
        "query": query,
        "sentence": sentence,
        "documents": documents,
        "matching_documents": matching,
    }


def gen_bm25_reference(corpus: dict, k: int = 10) -> dict:
    """T011 -- independent Python BM25, transcribed from tantivy 0.26.2 (report.md D-001).

    This is a *reimplementation from the documented formula*, not a second call into tantivy.
    That is what makes it a reference implementation under Principle II; a Python binding to the
    same engine would satisfy the letter of the rule and none of its purpose.
    """
    docs = corpus["documents"]
    n_docs = len(docs)

    tokenized = [analyze(d["text"]) for d in docs]
    fieldnorms = [len(t) for t in tokenized]
    # Only the per-document length is quantized; the average uses raw counts (bm25.rs:111 with
    # total_num_tokens accumulated one-per-token in postings_writer.rs).
    total_num_tokens = sum(fieldnorms)
    avg_fieldnorm = total_num_tokens / n_docs

    fieldnorm_ids = [fieldnorm_to_id(f) for f in fieldnorms]
    dequantized = [id_to_fieldnorm(i) for i in fieldnorm_ids]
    lossy = sum(1 for raw, deq in zip(fieldnorms, dequantized) if raw != deq)

    query_terms = analyze(corpus["query"])
    assert query_terms, "query analyzed to nothing"

    term_stats = []
    for term in query_terms:
        doc_freq = sum(1 for toks in tokenized if term in toks)
        assert doc_freq > 0, f"term {term!r} appears in no document"
        term_idf = idf(doc_freq, n_docs)
        term_stats.append({"term": term, "doc_freq": doc_freq, "idf": term_idf,
                           "weight": term_idf * (1.0 + K1)})

    scored = []
    for i, toks in enumerate(tokenized):
        tf_component = K1 * (1.0 - B + B * dequantized[i] / avg_fieldnorm)
        score = 0.0
        freqs = {}
        for stat in term_stats:
            tf = toks.count(stat["term"])
            if tf == 0:
                continue
            freqs[stat["term"]] = tf
            score += stat["weight"] * tf / (tf + tf_component)
        if score > 0.0:
            scored.append({
                "external_id": docs[i]["external_id"],
                "score": score,
                "field_len": fieldnorms[i],
                "field_len_dequantized": dequantized[i],
                "term_freq": freqs,
            })

    scored.sort(key=lambda h: (-h["score"], h["external_id"]))
    hits = scored[:k]
    for rank, hit in enumerate(hits):
        hit["rank"] = rank

    # Order-exactness has to be defensible, not asserted. If two adjacent top-k scores are closer
    # together than the tolerance we compare with, their order is not meaningfully determined and
    # the fixture would be testing float noise.
    gaps = [hits[i]["score"] - hits[i + 1]["score"] for i in range(len(hits) - 1)]
    min_gap = min(gaps) if gaps else float("inf")
    assert min_gap > SCORE_REL_TOL * hits[0]["score"] * 10, (
        f"top-{k} scores are too close to compare by order (min gap {min_gap})"
    )
    assert lossy > 0, (
        "no document has a lossy fieldnorm, so the quantization transcription is never exercised"
    )

    return {
        "schema_version": 1,
        "provenance": "python-independent-bm25",
        "transcribed_from": (
            "tantivy 0.26.2: src/query/bm25.rs (K1/B/idf/tf), src/fieldnorm/code.rs "
            "(FIELD_NORMS_TABLE), src/tokenizer/{simple_tokenizer,remove_long,tokenizer_manager}.rs"
        ),
        "analyzer": "default = SimpleTokenizer -> RemoveLongFilter(40 bytes, pre-lowercase) -> LowerCaser",
        "k1": K1, "b": B, "k": k,
        "query": corpus["query"],
        "score_rel_tol": SCORE_REL_TOL,
        "corpus_stats": {
            "n_docs": n_docs,
            "total_num_tokens": total_num_tokens,
            "avg_fieldnorm": avg_fieldnorm,
            "docs_with_lossy_fieldnorm": lossy,
        },
        "query_terms": term_stats,
        "min_top_k_score_gap": min_gap,
        "hits": hits,
    }


def fetch_and_verify_model(cache_dir: Path) -> dict:
    """T007 -- download the pinned revision and verify it three ways before anything trusts it."""
    from huggingface_hub import snapshot_download

    print(f"  fetching {MODEL_REPO}@{MODEL_REVISION[:12]} -> {cache_dir}")
    local = Path(snapshot_download(
        repo_id=MODEL_REPO, revision=MODEL_REVISION,
        allow_patterns=list(MODEL_FILES), local_dir=cache_dir,
    ))

    weights = local / WEIGHTS_FILE
    actual = weights.stat().st_size
    if actual != WEIGHTS_BYTES:
        raise SystemExit(
            f"{WEIGHTS_FILE} is {actual} bytes, expected exactly {WEIGHTS_BYTES} (FR-016). "
            "Hard error, never a warning -- the revision or the file is not what this spec pinned."
        )

    # dtype is READ from the safetensors header, not assumed (FR-033). Layout: 8-byte little-endian
    # header length, then that many bytes of JSON.
    with weights.open("rb") as fh:
        header_len = int.from_bytes(fh.read(8), "little")
        header = json.loads(fh.read(header_len))
    dtypes = {v["dtype"] for kk, v in header.items() if kk != "__metadata__"}
    f32_count = sum(1 for kk, v in header.items() if kk != "__metadata__" and v["dtype"] == "F32")
    if not {"F32"} <= dtypes:
        raise SystemExit(f"expected F32 weights, safetensors header reports {sorted(dtypes)}")

    digest = sha256_file(weights)
    print(f"  verified {WEIGHTS_BYTES} bytes, {f32_count} F32 tensors, sha256 {digest[:16]}...")

    config = json.loads((local / "config.json").read_text())
    return {
        "schema_version": 1,
        "repo": MODEL_REPO,
        "revision": MODEL_REVISION,
        "weights_file": WEIGHTS_FILE,
        "weights_bytes": WEIGHTS_BYTES,
        "weights_sha256": digest,
        "dtype": "f32",
        "f32_tensor_count": f32_count,
        "embedding_dim": EMBEDDING_DIM,
        "max_sequence_length": MAX_SEQ_LEN,
        "pooling": "mean",
        "normalize": True,
        "license": "apache-2.0",
        "config": {
            "hidden_size": config["hidden_size"],
            "num_hidden_layers": config["num_hidden_layers"],
            "num_attention_heads": config["num_attention_heads"],
            "vocab_size": config["vocab_size"],
            "max_position_embeddings": config["max_position_embeddings"],
        },
    }


def gen_tokens(model_dir: Path, sentence: str) -> dict:
    """T009 -- reference tokenization, with the baked-in 128 overridden to 256 (research D6)."""
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(model_dir / "tokenizer.json"))
    # tokenizer.json ships truncation.max_length=128 and padding Fixed(128). Python's
    # AutoTokenizer overrides these from tokenizer_config.json; a naive Rust `Tokenizer::from_file`
    # does not. Overriding on BOTH sides is what stops a tokenizer disagreement being misfiled as
    # an iOS embedding failure.
    tok.enable_truncation(max_length=MAX_SEQ_LEN)
    tok.enable_padding(length=MAX_SEQ_LEN, pad_id=0, pad_token="[PAD]", direction="right")

    enc = tok.encode(sentence)
    assert len(enc.ids) == MAX_SEQ_LEN, f"expected {MAX_SEQ_LEN} ids, got {len(enc.ids)}"
    assert len(enc.attention_mask) == MAX_SEQ_LEN
    assert set(enc.type_ids) == {0}, "single-segment sentence must have all-zero token_type_ids"

    return {
        "schema_version": 1,
        "sentence": sentence,
        "max_sequence_length": MAX_SEQ_LEN,
        "truncation_override": "max_length=256 (tokenizer.json ships 128)",
        "input_ids": list(enc.ids),
        "attention_mask": list(enc.attention_mask),
        "token_type_ids": list(enc.type_ids),
        "n_real_tokens": sum(enc.attention_mask),
    }


def gen_embedding(model_dir: Path, tokens: dict, seed: int) -> dict:
    """T010 -- attention-mask-weighted mean pooling then L2 normalization (research D8)."""
    import torch
    from transformers import AutoModel

    torch.manual_seed(seed)
    torch.set_num_threads(1)
    torch.set_grad_enabled(False)

    model = AutoModel.from_pretrained(str(model_dir), dtype=torch.float32)
    model.eval()

    input_ids = torch.tensor([tokens["input_ids"]], dtype=torch.long)
    attention_mask = torch.tensor([tokens["attention_mask"]], dtype=torch.long)
    token_type_ids = torch.tensor([tokens["token_type_ids"]], dtype=torch.long)

    out = model(input_ids=input_ids, attention_mask=attention_mask, token_type_ids=token_type_ids)
    hidden = out.last_hidden_state

    mask = attention_mask.unsqueeze(-1).to(hidden.dtype)
    pooled = (hidden * mask).sum(dim=1) / mask.sum(dim=1)
    normed = torch.nn.functional.normalize(pooled, p=2.0, dim=1)

    vector = [float(x) for x in normed[0].tolist()]
    assert len(vector) == EMBEDDING_DIM, f"expected {EMBEDDING_DIM} dims, got {len(vector)}"
    norm = math.sqrt(sum(x * x for x in vector))
    assert abs(norm - 1.0) < 1e-5, f"embedding is not L2-normalized (norm {norm})"

    return {
        "schema_version": 1,
        "sentence": tokens["sentence"],
        "pooling": "mean (attention-mask weighted)",
        "normalize": "l2",
        "dim": EMBEDDING_DIM,
        "cosine_min": COSINE_MIN,
        "max_abs_diff": MAX_ABS_DIFF,
        "torch_version": torch.__version__,
        "vector": vector,
    }


def ranking_placeholder(k: int = 10) -> dict:
    """``ExpectedRanking`` cannot exist until the Rust operations land (PR 2).

    Committed as an explicitly unusable placeholder rather than omitted, so that
    ``tests/fixtures_valid.rs`` can enforce the ``pending_host_mint`` XOR ``minted`` invariant and
    no test can ever pass against an empty ranking.
    """
    return {
        "schema_version": 1,
        "status": "pending_host_mint",
        "k": k,
        "writer_threads": 1,
        "writer_memory_budget_bytes": 15_000_000,
        "hits": [],
        "mint_command": "reference/gen_001_fixtures.py --emit-ranking",
        "blocked_by": "T027/T028 (PR 2) -- spike_index/spike_query must exist first",
    }


SECTIONS = ("model", "corpus", "bm25", "tokens", "embedding", "ranking")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--out", type=Path, default=REPO_ROOT / "reference/fixtures/001")
    ap.add_argument("--model-cache", type=Path, default=REPO_ROOT / "reference/models/001")
    ap.add_argument("--only", action="append", choices=SECTIONS,
                    help="generate only these sections (repeatable); default is all")
    ap.add_argument("--k", type=int, default=10, help="top-k for the BM25 fixtures")
    args = ap.parse_args()

    wanted = set(args.only) if args.only else set(SECTIONS)
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)

    print(f"gen_001_fixtures: seed={args.seed} out={out.relative_to(REPO_ROOT)}")

    if "model" in wanted:
        write_json(out / "model.json", fetch_and_verify_model(args.model_cache))

    corpus = None
    if {"corpus", "bm25", "tokens", "embedding"} & wanted:
        corpus = gen_corpus(args.seed)
        if "corpus" in wanted:
            write_json(out / "corpus.json", corpus)

    if "bm25" in wanted:
        write_json(out / "bm25_reference.json", gen_bm25_reference(corpus, k=args.k))

    tokens = None
    if {"tokens", "embedding"} & wanted:
        tokens = gen_tokens(args.model_cache, corpus["sentence"])
        if "tokens" in wanted:
            write_json(out / "tokens.json", tokens)

    if "embedding" in wanted:
        write_json(out / "embedding.json", gen_embedding(args.model_cache, tokens, args.seed))

    if "ranking" in wanted and not (out / "ranking.json").exists():
        write_json(out / "ranking.json", ranking_placeholder(k=args.k))

    # Manifest last: it hashes everything else, so a hand-edit to make a test pass goes red and
    # names the file that was touched (tests/fixtures_valid.rs enforces this).
    files = sorted(p for p in out.glob("*.json") if p.name != "manifest.json")
    write_json(out / "manifest.json", {
        "schema_version": 1,
        "seed": args.seed,
        "python_version": f"{sys.version_info[0]}.{sys.version_info[1]}.{sys.version_info[2]}",
        "generator_sha256": sha256_file(Path(__file__)),
        "files": {p.name: sha256_file(p) for p in files},
    })

    print("gen_001_fixtures: done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
