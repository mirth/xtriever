# xtriever — Python bindings

Hybrid retrieval for RAG — BM25 → dense → fusion → re-rank — as a Python package over the
Rust engine in this repository. Every hit, score, explanation and error you see from Python
is the engine's, bit for bit the same as from Rust or Swift; the package holds no retrieval
logic of its own.

## Install

The wheel is built from a checkout (Rust toolchain required to build, not to install):

```sh
cd python
uv venv .venv && uv pip install "maturin>=1.15,<2"       # or: python -m venv .venv && .venv/bin/pip install "maturin>=1.15,<2"
.venv/bin/maturin build --release                          # → ../target/wheels/xtriever-<version>-py3-none-<platform>.whl
uv pip install ../target/wheels/xtriever-*.whl             # into any Python ≥ 3.9 on macOS arm64 or Linux x86_64
```

The wheel tag is `py3-none-<platform>`: the module talks to the engine through `ctypes`, so
one wheel per platform serves every supported interpreter. Nothing generated is committed;
`maturin` runs the crate's own `uniffi-bindgen` and packs the shared library.

**Apple silicon note**: an x86_64 Python running under Rosetta (Homebrew under `/usr/local`
on some machines) cannot load the arm64 library — the import fails with ctypes' `OSError:
… incompatible architecture`. Use an arm64 interpreter (`/opt/homebrew/bin/python3.x`, or
`uv python install`).

## Models

The two pinned models are never bundled. Fetch them once with the repository's verified
script:

```sh
scripts/fetch-model.sh                                                       # the embedder  → reference/models/all-MiniLM-L6-v2
scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json      # the re-ranker → reference/models/ms-marco-MiniLM-L-6-v2
```

## A first search

```python
import xtriever

index = xtriever.IndexHandle.open(
    "path/to/index",                                   # a hybrid index directory
    "reference/models/all-MiniLM-L6-v2",               # the embedder (required)
    "reference/models/ms-marco-MiniLM-L-6-v2",         # the re-ranker (or None: fused order only)
    xtriever.LoadPath.MMAP,                            # both models memory-mapped
)
print(index.info())                                    # documents, format version, fingerprints, load times

response = index.search("why is the sky blue", xtriever.SearchOptions(k=5, explain=True))
for hit in response.hits:
    print(hit.external_id, hit.score, hit.rerank_score, hit.text[:80])
    e = hit.explain                                    # the seven pipeline features; None = not seen by that stage
    print("  bm25", e.bm25_score, e.bm25_rank, "dense", e.dense_score, e.dense_rank, "fused", e.fused,
          "rerank", e.rerank_score, e.rerank_rank)
print(response.stages)                                 # lexical/dense candidates, degradation, re-rank report
print(response.elapsed_ms, "ms")
```

`SearchOptions(k)` is enough; `depth`, `rerank_depth`, `max_time_ms`, `max_items` default to
the index's configuration, `strict` and `explain` to `False`. Under a time budget a stage that
runs out degrades to the previous stage's result and the report says so; with `strict=True`
it raises instead.

## Errors

Every engine error kind is its own class under one base:

```python
try:
    xtriever.IndexHandle.open("nope", embedder_dir, None, xtriever.LoadPath.MMAP)
except xtriever.XtrieverError.Corrupt as e:            # not a hybrid index
    print(e)
except xtriever.XtrieverError as e:                    # any other kind
    print(type(e).__name__, e)
```

Kinds: `Schema`, `InvalidQuery`, `UnknownField`, `DimensionMismatch`, `NotFound`, `Model`,
`Corrupt`, `FingerprintMismatch`, `BudgetExhausted`, `Io`, `Backend`. Which one is raised is
the engine's decision, unchanged from Rust.

## Threads

Calls release the interpreter lock for the duration of the engine call, so other Python
threads keep running during a search. Searches on one handle are serialised by the engine,
in call order; a time budget counts from the moment the call takes the handle.

## Building an index

*PR B of Feature 011 — filled in with the builder exports.*

## Tests

```sh
.venv/bin/pytest tests -q                    # everything, with the models and the fixture index on disk
.venv/bin/pytest tests -q -m "not models"    # the model-free subset (what CI runs)
```

The oracle is `swift/Xtriever/Tests/Fixtures/expected.json`, the goldens minted by the engine
from the 40-document fixture: every query is compared on score bits.

## Platforms

Built and tested on macOS arm64 and Linux x86_64. Other platforms build from source where
the Rust toolchain supports them, unverified. Python ≥ 3.9 by metadata; tested on 3.12 and
3.13.
