#!/usr/bin/env python
"""Feature 012 — the sparse-expansion spike (specs/012-sparse-spike).

Measures, in Python, what learned sparse expansions (OpenSearch's inference-free document
encoders) are worth in Xtriever's inverted index before any Rust is written: the engine's own
exported runs are the baselines, the 003 reference scorer is the oracle, and every variant of
research D5 becomes a run file with a report.

    sparse_spike.py pin      --repo R --out MANIFEST
    sparse_spike.py encode   --manifest M --dataset D [--device mps|cpu] [--batch 32]
    sparse_spike.py export   --dataset D
    sparse_spike.py score    --manifest M --dataset D --variant NAME [--scale 100] [--boost 1.0]
    sparse_spike.py all      --manifest M --dataset D
    sparse_spike.py summary

Nothing here ships: caches and runs live under target/, the manifests and the summary JSON
under the repository, the weights nowhere (scripts/fetch-model.sh fetches them by pin).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MODELS_DIR = REPO_ROOT / "reference" / "models"
BEIR_DIR = REPO_ROOT / "reference" / "datasets" / "beir"
CACHE_DIR = REPO_ROOT / "target" / "xt-sparse-cache"
RUNS_DIR = REPO_ROOT / "target" / "xt-sparse-runs"
SPEC_RUNS_DIR = REPO_ROOT / "specs" / "012-sparse-spike" / "runs"

MODEL_FILES = ["config.json", "tokenizer.json", "model.safetensors", "idf.json"]
DATASETS = ["scifact", "nfcorpus", "fiqa"]
DEPTH = 100
RRF_K = 60
BM25_K1, BM25_B = 1.2, 0.75
SHARD = 1_000


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# --------------------------------------------------------------------------------------------
# pin: the manifest in the 004 shape (research D1), fetched later by scripts/fetch-model.sh
# --------------------------------------------------------------------------------------------
def cmd_pin(args: argparse.Namespace) -> int:
    from huggingface_hub import HfApi, hf_hub_download

    api = HfApi()
    info = api.model_info(args.repo)
    revision = info.sha
    card = info.card_data or {}
    license_name = card.get("license") if isinstance(card, dict) else getattr(card, "license", None)
    basename = args.repo.split("/")[-1]
    activation = "log1p_log1p_relu" if "-v3-" in basename else "log1p_relu"
    files = []
    with tempfile.TemporaryDirectory() as tmp:
        for name in MODEL_FILES:
            local = Path(hf_hub_download(repo_id=args.repo, filename=name, revision=revision, cache_dir=tmp))
            files.append({"name": name, "bytes": local.stat().st_size, "sha256": sha256_file(local)})
            print(f"pin: {name} {files[-1]['bytes']} {files[-1]['sha256']}")
        # Parameter count from the safetensors header (the card says 67M for both models).
        st = next(p for p in Path(tmp).rglob("model.safetensors"))
        with st.open("rb") as f:
            n = int.from_bytes(f.read(8), "little")
            header = json.loads(f.read(n))
        params = 0
        for k, v in header.items():
            if k == "__metadata__":
                continue
            count = 1
            for d in v["shape"]:
                count *= d
            params += count
    manifest = {
        "schema_version": 1,
        "note": (
            "Feature 012 sparse-expansion spike: an inference-free document encoder (OpenSearch "
            "neural sparse), pinned by revision and file hash; fetched by scripts/fetch-model.sh, "
            "never committed. Query side: tokenizer + idf.json, no model call (research D1)."
        ),
        "repository": args.repo,
        "revision": revision,
        "local_dir": basename,
        "license": license_name,
        "parameters": params,
        "activation": activation,
        "files": files,
    }
    out = Path(args.out)
    out.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"pin: wrote {out} ({args.repo} @ {revision}, {params:,} parameters, license {license_name})")
    return 0



# --------------------------------------------------------------------------------------------
# Datasets and the 003 oracle
# --------------------------------------------------------------------------------------------
def scorer():
    """The 003 reference scorer, imported (research D2)."""
    sys.path.insert(0, str(REPO_ROOT / "reference"))
    import gen_003_fixtures as ref  # noqa: E402

    return ref


def read_jsonl(path: Path):
    with path.open("rb") as f:
        for line in f:
            line = line.strip()
            if line:
                yield json.loads(line)


def passage(doc: dict) -> str:
    """The harness's passage: `title + " " + text`, the title omitted when empty."""
    title = (doc.get("title") or "").strip()
    text = doc.get("text") or ""
    return f"{title} {text}" if title else text


def load_corpus(dataset: str) -> tuple[list[str], list[str]]:
    ids, texts = [], []
    for doc in read_jsonl(BEIR_DIR / dataset / "corpus.jsonl"):
        ids.append(doc["_id"])
        texts.append(passage(doc))
    return ids, texts


def load_qrels(dataset: str) -> dict[str, dict[str, int]]:
    return scorer().load_qrels_tsv(BEIR_DIR / dataset / "qrels" / "test.tsv")


def load_queries(dataset: str) -> tuple[list[str], list[str]]:
    """The judged queries, ascending id (string order, as the harness exports them)."""
    judged = set(load_qrels(dataset))
    pairs = [(q["_id"], q["text"]) for q in read_jsonl(BEIR_DIR / dataset / "queries.jsonl") if q["_id"] in judged]
    pairs.sort()
    return [q for q, _ in pairs], [t for _, t in pairs]


def read_run(path: Path) -> dict[str, list[str]]:
    return {rec["query_id"]: rec["doc_ids"] for rec in read_jsonl(path)}


def write_run(path: Path, run: dict[str, list[str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as f:
        for qid in sorted(run):
            f.write(json.dumps({"query_id": qid, "doc_ids": run[qid]}) + "\n")


def report_run(dataset: str, name: str, run: dict[str, list[str]], provenance: dict) -> dict:
    """Score a run with the 003 reference and write both files (data-model "Run", "Report")."""
    ref = scorer()
    out = RUNS_DIR / dataset
    write_run(out / f"{name}.jsonl", run)
    rep = ref.reference(load_qrels(dataset), run)
    rep["provenance"] = {"dataset": dataset, "name": name, **provenance}
    rep["empty_queries"] = sum(1 for v in run.values() if not v)
    (out / f"{name}.json").write_text(json.dumps(rep, indent=1, sort_keys=True) + "\n")
    print(f"{dataset:9s} {name:28s} ndcg@10 {rep['mean_ndcg_10']:.5f}  recall@100 {rep['mean_recall_100']:.5f}  (n={rep['scored_queries']}, empty={rep['empty_queries']})")
    return rep


# --------------------------------------------------------------------------------------------
# export: the engine's own runs, re-scored — SC-001
# --------------------------------------------------------------------------------------------
BASELINES = {
    "lexical": ("lexical-baseline-v1", "specs/003-eval-harness/baselines/lexical-baseline-v1.{d}.json"),
    "dense": ("dense-baseline-v1", "specs/004-dense-stage/baselines/dense-baseline-v1.{d}.json"),
    "hybrid": ("hybrid-baseline-v1", "specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.{d}.json"),
}


def cmd_export(args: argparse.Namespace) -> int:
    import subprocess

    d = args.dataset
    out = RUNS_DIR / d
    out.mkdir(parents=True, exist_ok=True)
    ref = scorer()
    qrels = load_qrels(d)
    for stage, (config, baseline) in BASELINES.items():
        run_path = out / f"engine-{stage}.jsonl"
        cmd = [
            "cargo", "run", "--release", "-p", "xtriever-eval", "--example", "beir", "--",
            "run", "--dataset", d, "--config", config, "--export-run", str(run_path),
            "--out", f"/tmp/xt-sparse-{d}-{stage}.json",
        ]
        if stage in ("dense", "hybrid"):
            cmd += ["--cache-dir", "target/xt-dense-cache"]
        if stage == "hybrid":
            cmd += ["--index-dir", f"target/xt-rerank-index/{d}"]
        print("export:", " ".join(cmd))
        subprocess.run(cmd, cwd=REPO_ROOT, check=True)
        run = read_run(run_path)
        got = ref.reference(qrels, run)
        want = json.loads((REPO_ROOT / baseline.format(d=d)).read_text())
        for key in ("mean_ndcg_10", "mean_recall_100"):
            delta = abs(got[key] - want[key])
            print(f"  engine-{stage} {key}: spike {got[key]:.10f} baseline {want[key]:.10f} |Δ|={delta:.2e}")
            if delta > 1e-6:
                sys.exit(f"export: engine-{stage} on {d} does not reproduce its committed baseline ({key})")
        if got["scored_queries"] != want["scored_queries"]:
            sys.exit(f"export: engine-{stage} on {d}: scored_queries {got['scored_queries']} != {want['scored_queries']}")
        got["provenance"] = {"dataset": d, "name": f"engine-{stage}", "config": config, "baseline": baseline.format(d=d)}
        (out / f"engine-{stage}.json").write_text(json.dumps(got, indent=1, sort_keys=True) + "\n")
        print(f"  engine-{stage}: reproduces {baseline.format(d=d)} (SC-001)")
    return 0


# --------------------------------------------------------------------------------------------
# The encoder (research D1) — the model card's recipe
# --------------------------------------------------------------------------------------------
class Loaded:
    """A pinned model, its tokenizer, the IDF vector and the recipe's constants."""

    def __init__(self, manifest: dict, device: str):
        import numpy as np
        import torch
        from transformers import AutoModelForMaskedLM, AutoTokenizer

        self.manifest = manifest
        self.local_dir = manifest["local_dir"]
        self.model_key = f"{manifest['local_dir']}@{manifest['revision'][:8]}"
        self.activation = manifest["activation"]
        model_dir = MODELS_DIR / self.local_dir
        for f in manifest["files"]:
            p = model_dir / f["name"]
            if not p.exists() or p.stat().st_size != f["bytes"] or sha256_file(p) != f["sha256"]:
                sys.exit(f"load_model: {p} does not match the manifest — run scripts/fetch-model.sh --manifest …")
        self.tokenizer = AutoTokenizer.from_pretrained(str(model_dir))
        self.model = AutoModelForMaskedLM.from_pretrained(str(model_dir)).eval()
        self.device = device
        self.model.to(device)
        self.vocab_size = len(self.tokenizer)
        config = json.loads((model_dir / "config.json").read_text())
        self.max_length = int(config.get("max_position_embeddings", 512))
        self.special_ids = sorted({self.tokenizer.convert_tokens_to_ids(t) for t in self.tokenizer.special_tokens_map.values()})
        idf = json.loads((model_dir / "idf.json").read_text())
        vec = np.zeros(self.vocab_size, dtype=np.float32)
        for token, weight in idf.items():
            vec[self.tokenizer.convert_tokens_to_ids(token)] = weight
        vec[self.special_ids] = 0.0
        self.idf = vec
        self.torch = torch


def load_model(manifest_path: Path, device: str | None = None) -> Loaded:
    import torch

    manifest = json.loads(Path(manifest_path).read_text())
    if device is None:
        device = "mps" if torch.backends.mps.is_available() else "cpu"
    return Loaded(manifest, device)


def id_to_token(loaded: Loaded) -> list[str]:
    return loaded.tokenizer.convert_ids_to_tokens(list(range(loaded.vocab_size)))


class Encoded:
    """Sparse vectors as lists of (indices, data) per input, plus the truncation count."""

    def __init__(self):
        self.indices: list = []
        self.data: list = []
        self.truncated = 0


def encode_documents(loaded: Loaded, texts: list[str], batch: int = 8) -> Encoded:
    """logits → max over tokens (attention-masked) → the model's activation → special tokens
    zeroed (research D1, the card's `get_sparse_vector`)."""
    import numpy as np

    torch = loaded.torch
    tok = loaded.tokenizer
    out = Encoded()
    # Truncation count: the untruncated length against the model's window.
    lengths = [len(ids) for ids in tok(texts, truncation=False, add_special_tokens=True)["input_ids"]]
    out.truncated = sum(1 for n in lengths if n > loaded.max_length)
    special = torch.tensor(loaded.special_ids, device=loaded.device)
    with torch.inference_mode():
        for start in range(0, len(texts), batch):
            chunk = texts[start : start + batch]
            feature = tok(chunk, padding=True, truncation=True, max_length=loaded.max_length, return_tensors="pt", return_token_type_ids=False)
            feature = {k: v.to(loaded.device) for k, v in feature.items()}
            logits = loaded.model(**feature)[0]
            values, _ = torch.max(logits * feature["attention_mask"].unsqueeze(-1), dim=1)
            values = torch.log1p(torch.relu(values))
            if loaded.activation == "log1p_log1p_relu":
                values = torch.log1p(values)
            values[:, special] = 0
            values = values.float().cpu().numpy()
            del logits, feature
            if loaded.device == "mps":
                torch.mps.empty_cache()
            for row in values:
                nz = np.nonzero(row > 0)[0].astype(np.int32)
                out.indices.append(nz)
                out.data.append(row[nz].astype(np.float32))
    return out


def encode_queries(loaded: Loaded, texts: list[str]) -> Encoded:
    """The inference-free query side: each distinct token id present → idf[token]; no model."""
    import numpy as np

    out = Encoded()
    enc = loaded.tokenizer(texts, truncation=True, max_length=loaded.max_length, add_special_tokens=True)["input_ids"]
    special = set(loaded.special_ids)
    for ids in enc:
        keep = sorted({i for i in ids if i not in special and loaded.idf[i] > 0})
        out.indices.append(np.array(keep, dtype=np.int32))
        out.data.append(loaded.idf[keep].astype(np.float32) if keep else np.zeros(0, dtype=np.float32))
    return out


# --------------------------------------------------------------------------------------------
# encode: shards under target/ (research D4) and the costs record
# --------------------------------------------------------------------------------------------
def cache_dir(loaded: Loaded, dataset: str) -> Path:
    return CACHE_DIR / loaded.model_key / dataset


def save_shard(path: Path, ids: list[str], enc: Encoded, first: int, last: int, wall_s: float | None = None) -> None:
    """One shard: the CSR pieces plus its own metadata (documents, wall time, truncated count),
    so a resumed run aggregates every shard's cost rather than the current run's (review 1 #1)."""
    import numpy as np

    rows = enc.indices[first:last]
    indptr = np.zeros(len(rows) + 1, dtype=np.int64)
    indptr[1:] = np.cumsum([len(r) for r in rows])
    part = path.with_name(path.stem + ".part.npz")  # np.savez appends .npz to any other name
    np.savez(
        part,
        ids=np.array(ids[first:last], dtype=object),
        indptr=indptr,
        indices=np.concatenate(rows) if rows else np.zeros(0, dtype=np.int32),
        data=np.concatenate(enc.data[first:last]) if rows else np.zeros(0, dtype=np.float32),
        meta=np.array([json.dumps({"documents": last - first, "wall_s": wall_s, "truncated": enc.truncated})], dtype=object),
    )
    part.replace(path)


def shard_meta(z) -> dict:
    """A shard's metadata; shards written before review 1 #1 have none and are re-encoded."""
    if "meta" in z.files:
        return json.loads(str(z["meta"][0]))
    return {}


def cmd_encode(args: argparse.Namespace) -> int:
    import time

    import numpy as np

    loaded = load_model(Path(args.manifest), args.device)
    d = args.dataset
    out = cache_dir(loaded, d)
    out.mkdir(parents=True, exist_ok=True)
    ids, texts = load_corpus(d)
    nnz, metas = [], []
    shards = range(0, len(ids), SHARD)
    for n, start in enumerate(shards):
        path = out / f"docs-{n:05d}.npz"
        end = min(start + SHARD, len(ids))
        if path.exists():
            z = np.load(path, allow_pickle=True)
            meta = shard_meta(z)
            if meta.get("wall_s") is not None:
                nnz.extend(np.diff(z["indptr"]).tolist())
                metas.append(meta)
                print(f"encode: {path.name} cached")
                continue
            print(f"encode: {path.name} has no metadata (pre-review shard) — re-encoding")
        t0 = time.perf_counter()
        enc = encode_documents(loaded, texts[start:end], batch=args.batch)
        dt = time.perf_counter() - t0
        save_shard(path, ids[start:end], enc, 0, end - start, wall_s=dt)
        nnz.extend(len(r) for r in enc.indices)
        metas.append({"documents": end - start, "wall_s": dt, "truncated": enc.truncated})
        print(f"encode: {path.name} {end - start} docs in {dt:.1f} s ({(end - start) / dt:.1f} docs/s, {loaded.device}, {loaded.torch.get_num_threads()} threads)")
    wall = sum(m["wall_s"] for m in metas)
    timed_docs = sum(m["documents"] for m in metas)
    truncated = sum(m["truncated"] for m in metas)
    qids, qtexts = load_queries(d)
    qenc = encode_queries(loaded, qtexts)
    save_shard(out / "queries.npz", qids, qenc, 0, len(qids))
    qnnz = [len(r) for r in qenc.indices]
    record_path = RUNS_DIR / d / f"costs-{loaded.model_key}.json"
    record_path.parent.mkdir(parents=True, exist_ok=True)
    # Aggregated over every shard's own metadata — a resumed run reports the whole corpus.
    record = {
        "documents": len(ids), "timed_documents": timed_docs, "wall_s": round(wall, 1),
        "docs_per_s": round(timed_docs / wall, 1) if wall > 0 else None, "batch": args.batch,
        "device": loaded.device, "threads": loaded.torch.get_num_threads(), "truncated_docs": truncated,
        "shards": len(metas),
    }
    record.update({
        "model_key": loaded.model_key, "dataset": d,
        "nnz_doc": {"mean": float(np.mean(nnz)), "p95": float(np.percentile(nnz, 95)), "max": int(np.max(nnz))},
        "nnz_query": {"mean": float(np.mean(qnnz)), "p95": float(np.percentile(qnnz, 95)), "max": int(np.max(qnnz))},
        "empty_queries": int(sum(1 for n in qnnz if n == 0)), "queries": len(qids),
        "max_length": loaded.max_length, "idf_entries": int(np.count_nonzero(loaded.idf)),
        "model": {k: loaded.manifest[k] for k in ("repository", "revision", "license", "parameters")},
        "model_bytes": sum(f["bytes"] for f in loaded.manifest["files"]),
    })
    record_path.write_text(json.dumps(record, indent=1, sort_keys=True) + "\n")
    print(f"encode: {d} nnz/doc mean {record['nnz_doc']['mean']:.1f} p95 {record['nnz_doc']['p95']:.0f}; nnz/query mean {record['nnz_query']['mean']:.1f}; wrote {record_path}")
    return 0


def load_cached(loaded: Loaded, dataset: str):
    """(doc ids, CSR docs × vocab), (query ids, CSR queries × vocab)."""
    import numpy as np
    import scipy.sparse as sp

    out = cache_dir(loaded, dataset)
    shards = sorted(out.glob("docs-*.npz"))
    if not shards:
        sys.exit(f"load_cached: no encodings under {out} — run `encode` first")

    def read(paths):
        ids, mats = [], []
        for p in paths:
            z = np.load(p, allow_pickle=True)
            n = len(z["indptr"]) - 1
            ids.extend(z["ids"].tolist())
            mats.append(sp.csr_matrix((z["data"], z["indices"], z["indptr"]), shape=(n, loaded.vocab_size)))
        return np.array(ids, dtype=object), sp.vstack(mats).tocsr()

    return read(shards), read([out / "queries.npz"])


# --------------------------------------------------------------------------------------------
# Scoring (research D5)
# --------------------------------------------------------------------------------------------
def top_k(ids, scores, k: int = DEPTH) -> list[str]:
    """Top-k by score, zeros excluded, ties by ascending doc id."""
    import numpy as np

    keep = np.nonzero(scores > 0)[0]
    if keep.size == 0:
        return []
    order = np.lexsort((ids[keep], -scores[keep]))
    return [str(ids[keep][i]) for i in order[:k]]


def dot_run(doc_ids, docs, qids, queries, scale: int | None = None) -> dict[str, list[str]]:
    import numpy as np

    d = docs
    if scale:
        d = docs.copy()
        d.data = np.round(d.data * scale) / scale
        d.eliminate_zeros()
    run = {}
    for i, qid in enumerate(qids):
        scores = (d @ queries[i].T).toarray().ravel()
        run[str(qid)] = top_k(doc_ids, scores)
    return run


def bm25_over_field(tf, query_terms, k1: float = BM25_K1, b: float = BM25_B):
    """BM25 over a CSR of integer term frequencies (documents × terms); the Lucene/tantivy IDF."""
    import numpy as np

    n = tf.shape[0]
    lengths = np.asarray(tf.sum(axis=1)).ravel().astype(np.float64)
    avg = lengths.mean() if n else 1.0
    df = np.asarray((tf > 0).sum(axis=0)).ravel().astype(np.float64)
    terms = [t for t in dict.fromkeys(query_terms) if t < tf.shape[1]]
    if not terms:
        return np.zeros(n)
    sub = tf[:, terms].toarray().astype(np.float64)  # n × |terms|
    idf = np.log(1 + (n - df[terms] + 0.5) / (df[terms] + 0.5))
    norm = k1 * (1 - b + b * lengths / avg)
    scores = (sub * (k1 + 1) / (sub + norm[:, None])) * idf[None, :]
    scores[sub == 0] = 0.0
    return scores.sum(axis=1)


def bm25x_run(doc_ids, docs, qids, queries, scale: int) -> tuple[dict[str, list[str]], object]:
    """The expansion field as a BM25 field: tf = round(w × scale); query = its token ids."""
    import numpy as np

    tf = docs.copy()
    tf.data = np.round(tf.data * scale)
    tf.eliminate_zeros()
    run = {}
    for i, qid in enumerate(qids):
        run[str(qid)] = top_k(doc_ids, bm25_over_field(tf, queries[i].indices.tolist()))
    return run, tf


_WORD = None


def text_tokens(text: str) -> list[str]:
    """The spike's approximation of the engine's `standard_en`: split on non-alphanumerics,
    lower-case, Snowball English stem, drop tokens over 40 characters (research D5)."""
    import re

    import snowballstemmer

    global _WORD
    if _WORD is None:
        _WORD = (re.compile(r"[0-9A-Za-z]+"), snowballstemmer.stemmer("english"))
    pattern, stemmer = _WORD
    return [stemmer.stemWord(t) for t in (w.lower() for w in pattern.findall(text)) if len(t) <= 40]


def text_bm25_matrix(texts: list[str]):
    """CSR of term frequencies over the spike's text tokens, plus the term → column map."""
    import numpy as np
    import scipy.sparse as sp

    vocab: dict[str, int] = {}
    rows, cols, vals = [], [], []
    for i, text in enumerate(texts):
        counts: dict[int, int] = {}
        for t in text_tokens(text):
            j = vocab.setdefault(t, len(vocab))
            counts[j] = counts.get(j, 0) + 1
        for j, c in counts.items():
            rows.append(i)
            cols.append(j)
            vals.append(c)
    tf = sp.csr_matrix((np.array(vals, dtype=np.float32), (rows, cols)), shape=(len(texts), len(vocab)))
    return tf, vocab


def rrf(lists: list[list[str]], k: int = RRF_K) -> list[tuple[str, float]]:
    scores: dict[str, float] = {}
    for ranked in lists:
        for rank, did in enumerate(ranked[:DEPTH]):
            scores[did] = scores.get(did, 0.0) + 1.0 / (k + rank + 1)
    return sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))[:DEPTH]


def rrf_run(runs: list[dict[str, list[str]]]) -> dict[str, list[str]]:
    qids = set().union(*(r.keys() for r in runs))
    return {q: [d for d, _ in rrf([r.get(q, []) for r in runs])] for q in qids}


VARIANTS = ["dot", "dot-q10", "dot-q100", "dot-q1000", "bm25x", "bm25x+text-b0.5", "bm25x+text-b1.0", "bm25x+text-b2.0",
            "rrf-lex+dot", "rrf-dense+dot", "rrf-lex+dense+dot", "rrf-lex+bm25x", "rrf-dense+bm25x", "rrf-lex+dense+bm25x"]


class Variants:
    """Everything `score` needs for one model on one dataset, computed lazily and once."""

    def __init__(self, loaded: Loaded, dataset: str, scale: int):
        self.loaded, self.dataset, self.scale = loaded, dataset, scale
        (self.doc_ids, self.docs), (self.qids, self.queries) = load_cached(loaded, dataset)
        self.qids = [str(q) for q in self.qids]
        self._text = None
        self._runs: dict[str, dict[str, list[str]]] = {}
        self.text_overlap = None

    def engine(self, stage: str) -> dict[str, list[str]]:
        path = RUNS_DIR / self.dataset / f"engine-{stage}.jsonl"
        if not path.exists():
            sys.exit(f"missing {path} — run `export --dataset {self.dataset}` first")
        run = read_run(path)
        if set(run) != set(self.qids):
            sys.exit(f"{path}: its query set differs from the dataset's judged queries")
        return run

    def text(self):
        if self._text is None:
            import numpy as np

            _, texts = load_corpus(self.dataset)
            tf, vocab = text_bm25_matrix(texts)
            _, qtexts = load_queries(self.dataset)
            qterms = [[vocab[t] for t in text_tokens(q) if t in vocab] for q in qtexts]
            self._text = (tf, qterms)
            # The tokenizer-difference figure: top-10 Jaccard against the engine's lexical run.
            spike = {qid: top_k(self.doc_ids, bm25_over_field(tf, terms)) for qid, terms in zip(self.qids, qterms)}
            engine = self.engine("lexical")
            j = [len(set(spike[q][:10]) & set(engine[q][:10])) / max(1, len(set(spike[q][:10]) | set(engine[q][:10]))) for q in self.qids]
            self.text_overlap = float(np.mean(j))
            self._runs["text-bm25"] = spike
        return self._text

    def run(self, name: str) -> dict[str, list[str]]:
        if name in self._runs:
            return self._runs[name]
        import numpy as np

        if name == "dot":
            r = dot_run(self.doc_ids, self.docs, self.qids, self.queries)
        elif name.startswith("dot-q"):
            r = dot_run(self.doc_ids, self.docs, self.qids, self.queries, scale=int(name[5:]))
        elif name == "bm25x":
            r, _ = bm25x_run(self.doc_ids, self.docs, self.qids, self.queries, self.scale)
        elif name.startswith("bm25x+text-b"):
            beta = float(name.split("-b")[1])
            tf_text, qterms = self.text()
            tf_x = self.docs.copy()
            tf_x.data = np.round(tf_x.data * self.scale)
            tf_x.eliminate_zeros()
            r = {}
            for i, qid in enumerate(self.qids):
                s = bm25_over_field(tf_text, qterms[i]) + beta * bm25_over_field(tf_x, self.queries[i].indices.tolist())
                r[qid] = top_k(self.doc_ids, s)
        elif name.startswith("rrf-"):
            parts = name[4:].split("+")
            sources = {"lex": lambda: self.engine("lexical"), "dense": lambda: self.engine("dense")}
            r = rrf_run([sources[p]() if p in sources else self.run(p) for p in parts])
        else:
            sys.exit(f"unknown variant {name}")
        self._runs[name] = r
        return r


def effective_scale(variant: str, default: int) -> int | None:
    """The quantisation scale a variant actually used: `dot-qS` carries its own; `dot` none;
    the BM25-field variants use `--scale` (review 1 #2)."""
    if variant.startswith("dot-q"):
        return int(variant[5:])
    if variant == "dot" or variant.startswith("rrf-") and variant.endswith("+dot"):
        return None
    return default


def cmd_score(args: argparse.Namespace) -> int:
    loaded = load_model(Path(args.manifest), args.device)
    v = Variants(loaded, args.dataset, args.scale)
    names = VARIANTS if args.variant in (None, "all") else [args.variant]
    for name in names:
        run = v.run(name)
        report_run(args.dataset, f"{name}@{loaded.model_key}", run, {"model_key": loaded.model_key, "variant": name, "scale": effective_scale(name, args.scale), "rrf_k": RRF_K, "bm25": {"k1": BM25_K1, "b": BM25_B}})
    if v.text_overlap is not None:
        report_run(args.dataset, "text-bm25@spike", v._runs["text-bm25"], {"variant": "text-bm25", "note": "the spike's own text BM25 (standard_en approximation)", "top10_jaccard_vs_engine_lexical": v.text_overlap})
        print(f"{args.dataset:9s} spike text-BM25 vs engine-lexical: mean top-10 Jaccard {v.text_overlap:.3f}")
    return 0


def cmd_all(args: argparse.Namespace) -> int:
    loaded = load_model(Path(args.manifest), args.device)
    if not (cache_dir(loaded, args.dataset) / "queries.npz").exists():
        cmd_encode(args)
    args.variant = "all"
    return cmd_score(args)


# --------------------------------------------------------------------------------------------
# summary: the tables and the FR-011 verdict
# --------------------------------------------------------------------------------------------
def cmd_summary(args: argparse.Namespace) -> int:
    summary: dict = {"datasets": {}, "costs": {}, "quantisation": {}, "text_overlap": {}}
    for d in DATASETS:
        rows = {}
        for path in sorted((RUNS_DIR / d).glob("*.json")):
            if path.name.startswith("costs-"):
                summary["costs"].setdefault(path.stem[6:], {})[d] = json.loads(path.read_text())
                continue
            rep = json.loads(path.read_text())
            rows[path.stem] = {"ndcg_10": rep["mean_ndcg_10"], "recall_100": rep["mean_recall_100"], "scored_queries": rep["scored_queries"], "empty_queries": rep.get("empty_queries", 0)}
            if path.stem == "text-bm25@spike":
                summary["text_overlap"][d] = rep["provenance"].get("top10_jaccard_vs_engine_lexical")
        summary["datasets"][d] = rows
    models = sorted({k.split("@", 1)[1] for d in summary["datasets"].values() for k in d if "@" in k and not k.endswith("@spike")})
    # Metrics table.
    lines = ["| variant | " + " | ".join(f"{d} nDCG@10 / R@100" for d in DATASETS) + " |", "|---|" + "---|" * len(DATASETS)]

    def cell(d, key):
        r = summary["datasets"][d].get(key)
        return f"{r['ndcg_10']:.4f} / {r['recall_100']:.4f}" if r else "—"

    for stage in ("engine-lexical", "engine-dense", "engine-hybrid"):
        lines.append(f"| {stage} | " + " | ".join(cell(d, stage) for d in DATASETS) + " |")
    lines.append("| text-bm25 (spike's own) | " + " | ".join(cell(d, "text-bm25@spike") for d in DATASETS) + " |")
    for m in models:
        for v in VARIANTS:
            lines.append(f"| {v} @ {m} | " + " | ".join(cell(d, f"{v}@{m}") for d in DATASETS) + " |")
    print("\n".join(lines))
    # Quantisation.
    print("\n| model | dataset | dot | ×10 loss | ×100 loss | ×1000 loss | smallest scale ≤ 0.1 pt |\n|---|---|---|---|---|---|---|")
    for m in models:
        for d in DATASETS:
            base = summary["datasets"][d].get(f"dot@{m}")
            if not base:
                continue
            losses = {}
            for s in (10, 100, 1000):
                q = summary["datasets"][d].get(f"dot-q{s}@{m}")
                losses[s] = (base["ndcg_10"] - q["ndcg_10"]) * 100 if q else None
            ok = [s for s in (10, 100, 1000) if losses[s] is not None and losses[s] <= 0.1]
            summary["quantisation"].setdefault(m, {})[d] = {"loss_points": losses, "smallest_ok": min(ok) if ok else None}
            print(f"| {m} | {d} | {base['ndcg_10']:.4f} | " + " | ".join("—" if losses[s] is None else f"{losses[s]:+.3f}" for s in (10, 100, 1000)) + f" | {min(ok) if ok else 'none'} |")
    # Costs.
    print("\n| model | dataset | docs | docs/s | device | threads | nnz/doc mean / p95 / max | nnz/query mean / p95 | truncated | empty queries |\n|---|---|---|---|---|---|---|---|---|---|")
    for m, per in summary["costs"].items():
        for d in DATASETS:
            c = per.get(d)
            if not c:
                continue
            print(f"| {m} | {d} | {c.get('documents', '—')} | {c.get('docs_per_s', '—')} | {c.get('device', '—')} | {c.get('threads', '—')} | {c['nnz_doc']['mean']:.0f} / {c['nnz_doc']['p95']:.0f} / {c['nnz_doc']['max']} | {c['nnz_query']['mean']:.1f} / {c['nnz_query']['p95']:.0f} | {c.get('truncated_docs', '—')} | {c['empty_queries']} |")
    print("\ntext-BM25 top-10 Jaccard vs engine-lexical:", {d: round(v, 3) for d, v in summary["text_overlap"].items() if v is not None})
    # FR-011.
    verdicts = {}
    for m in models:
        wins = sum(1 for d in DATASETS if f"dot@{m}" in summary["datasets"][d] and summary["datasets"][d][f"dot@{m}"]["ndcg_10"] > summary["datasets"][d]["engine-lexical"]["ndcg_10"])
        complete = all(f"rrf-lex+dense+dot@{m}" in summary["datasets"][d] and "engine-hybrid" in summary["datasets"][d] for d in DATASETS)
        if complete:
            fused = sum(summary["datasets"][d][f"rrf-lex+dense+dot@{m}"]["ndcg_10"] for d in DATASETS) / 3
            hybrid = sum(summary["datasets"][d]["engine-hybrid"]["ndcg_10"] for d in DATASETS) / 3
            dot_mean = sum(summary["datasets"][d][f"dot@{m}"]["ndcg_10"] for d in DATASETS) / 3
            bm25x_mean = sum(summary["datasets"][d][f"bm25x@{m}"]["ndcg_10"] for d in DATASETS) / 3 if all(f"bm25x@{m}" in summary["datasets"][d] for d in DATASETS) else None
        else:
            fused = hybrid = dot_mean = bm25x_mean = None
        go = complete and wins >= 2 and fused > hybrid
        verdicts[m] = {"dot_beats_lexical_on": wins, "fused_mean": fused, "hybrid_mean": hybrid, "dot_mean": dot_mean, "bm25x_mean": bm25x_mean, "go": bool(go),
                       "scoring": ("bm25x" if bm25x_mean is not None and dot_mean is not None and dot_mean - bm25x_mean <= 0.005 else "dot")}
        print(f"\nFR-011 {m}: dot beats engine-lexical on {wins}/3; rrf-lex+dense+dot mean {fused if fused is None else round(fused, 4)} vs engine-hybrid mean {hybrid if hybrid is None else round(hybrid, 4)} → {'GO' if go else 'NO-GO'}; scoring {verdicts[m]['scoring']}")
    summary["verdicts"] = verdicts
    SPEC_RUNS_DIR.mkdir(parents=True, exist_ok=True)
    (SPEC_RUNS_DIR / "summary.json").write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    print(f"\nsummary: wrote {SPEC_RUNS_DIR / 'summary.json'}")
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)

    s = sub.add_parser("pin", help="write a pinned manifest for a model repository")
    s.add_argument("--repo", required=True)
    s.add_argument("--out", required=True)
    s.set_defaults(func=cmd_pin)

    needs = {"encode": ("manifest", "dataset"), "export": ("dataset",), "score": ("manifest", "dataset", "variant"), "all": ("manifest", "dataset"), "summary": ()}
    for name, func in (("encode", cmd_encode), ("export", cmd_export), ("score", cmd_score), ("all", cmd_all), ("summary", cmd_summary)):
        s = sub.add_parser(name)
        s.add_argument("--manifest", required="manifest" in needs[name])
        s.add_argument("--dataset", choices=DATASETS, required="dataset" in needs[name])
        s.add_argument("--variant", required="variant" in needs[name], help="a research-D5 variant name, or `all`")
        s.add_argument("--device", choices=["mps", "cpu"])
        s.add_argument("--batch", type=int, default=8)  # the MLM logits are batch × tokens × vocab: 8 × 512 × 30,522 × 4 B ≈ 500 MB
        s.add_argument("--scale", type=int, default=100)
        s.add_argument("--boost", type=float, default=1.0)
        s.set_defaults(func=func)

    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
