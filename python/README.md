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
scripts/fetch-model.sh --manifest reference/models/manifest-q8.json          # the embedder  → reference/models/all-MiniLM-L6-v2-q8
scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json   # the re-ranker → reference/models/ms-marco-MiniLM-L-6-v2-q8
```

These are the eight-bit artefacts, the default since Feature 026 (the fixture index and the
tests expect them). The float ones (`scripts/fetch-model.sh`, and with
`--manifest reference/models/manifest-rerank.json`) load through the same call; an index
records which embedder built it and refuses the other.

## A first search

```python
import xtriever

index = xtriever.IndexHandle.open(
    "path/to/index",                                   # a hybrid index directory
    "reference/models/all-MiniLM-L6-v2-q8",            # the embedder (required)
    "reference/models/ms-marco-MiniLM-L-6-v2-q8",      # the re-ranker (or None: fused order only)
    xtriever.LoadPath.MMAP,                            # both models memory-mapped
)
print(index.info())                                    # documents, format version, fingerprints, load times

response = index.search("why is the sky blue", xtriever.SearchOptions(k=5, explain=True))
for hit in response.hits:
    print(hit.external_id, hit.score, hit.rerank_score, hit.text[:80])
    e = hit.explain                                    # the eight pipeline features; None = not seen by that stage
    print("  bm25", e.bm25_score, e.bm25_rank, "dense", e.dense_score, e.dense_rank, "fused", e.fused,
          "rerank", e.rerank_score, e.rerank_rank, e.rerank_combined)
print(response.stages)                                 # lexical/dense candidates, degradation, re-rank report
print(response.elapsed_ms, "ms")
```

`SearchOptions(k)` is enough; `depth`, `rerank_depth`, `rerank_mode`, `max_time_ms`,
`max_items` default to the index's configuration, `strict` and `explain` to `False`. Under a
time budget a stage that runs out degrades to the previous stage's result and the report
says so; with `strict=True` it raises instead.

**Re-ranking order.** By default the re-ranked head is ordered by
`0.5 · minmax(fused score) + 0.5 · minmax(cross-encoder score)` — the cross-encoder informs
the fused order rather than replacing it (`RerankMode.INTERPOLATE(alpha=0.5)`, recorded in the
index; `info().rerank_mode`). Measured on the BEIR sets this scores +1.45 mean nDCG@10 points
over the earlier replace-order rule at the same cost, and never below it on any set. The
earlier order is one option away, per search or per index:

```python
index.search("why is the sky blue", xtriever.SearchOptions(k=5, rerank_mode=xtriever.RerankMode.REPLACE()))
xtriever.IndexConfig(fields=[...], dense_fields=[...], rerank_mode=xtriever.RerankMode.REPLACE())   # recorded at build
```

`hit.explain.rerank_combined` is the score the head was ordered by (`None` under `REPLACE`).

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
threads keep running during a search. Calls on one handle run one at a time — a lock, not a
queue, so concurrent callers are not ordered; a time budget counts from the moment the call
takes the handle.

## Building an index

The same handle builds. A schema names the fields (text fields carry an analyzer id the
engine knows — `"standard"`, `"standard_en"`), the dense fields are the text fields joined
into the passage the embedder sees, and the depths default to the engine's (100 candidates,
RRF k 60, re-rank depth 20).

Index **one joined text field for BM25** — the title and the body in a single `contents`
field, boost 1.0 — rather than a boosted `title` field beside `text`: on the BEIR sets the
one-field layout scores +5.9 nDCG@10 points on SciFact and +1.1 on NFCorpus over
`title` × 2.0 + `text` (a boosted short field lets one title term outweigh several body
matches; Feature 013's report has the numbers). Keep a separate `title` field only if a
caller needs it on its own, and then unboosted:

```python
import xtriever
from xtriever import Document, FieldDef, FieldKind, FieldValue, IndexConfig

config = IndexConfig(
    fields=[
        FieldDef(name="contents", kind=FieldKind.TEXT(analyzer="standard_en")),
        FieldDef(name="source", kind=FieldKind.KEYWORD()),
    ],
    dense_fields=["contents"],
)
index = xtriever.IndexHandle.create("path/to/new-index", config, embedder_dir, reranker_dir, xtriever.LoadPath.MMAP)

index.add([
    Document(external_id="doc-1", fields={
        "contents": FieldValue.TEXT("Why the sky is blue Rayleigh scattering …"),
        "source": FieldValue.KEYWORD("notes"),
    }),
    Document(external_id="doc-1#1", fields={"contents": FieldValue.TEXT("Why the sky is blue …"), "source": FieldValue.KEYWORD("notes")},
             chunk=xtriever.ChunkInfo(parent="doc-1", ordinal=1, byte_start=120, byte_end=480)),
])
index.commit()                                   # staged changes become searchable only here
index.search("blue sky", xtriever.SearchOptions(k=3))
```

- **Replace**: `add` a document under a known id; the old version is searched until `commit`.
- **Delete**: `index.delete(["doc-1"])`, then `commit`; unknown ids are ignored. `contains(id)`
  reports the committed view.
- **Pre-computed vectors**: `index.add_embedded(docs, vectors)` with one vector per document,
  the embedder's width (384 for the pinned model) — `DimensionMismatch` otherwise, `Schema`
  when the counts differ.
- **Reopen and extend**: `IndexHandle.open(...)` on an existing index is writable when the
  directory can be locked; a directory the process cannot write opens read-only and every
  write raises `XtrieverError.Io` ("read-only index"). A **sparse** index (below) reopens
  search-only: adding to it raises `XtrieverError.Model`, because only the handle `create`
  returned holds the encoder.
- **Ship**: `index.merge()` commits, compacts the dense vectors (live rows only) and folds the
  lexical stage into one segment; after deletes or replacements the lexical statistics move
  across it, so fused scores can change (dense scores never do).
- **Refusals** are the engine's: a non-empty directory at `create` is `Corrupt`; a dense
  field missing from the schema, a non-text dense field, an unknown analyzer or an empty
  external id is `Schema`.

An index built here is the engine's index: the test suite builds the 40-document fixture
from Python and checks every golden query against it, score bits included.

### Sparse lexical expansion (optional)

An index can be built **sparse**: each passage is expanded by the pinned sparse document
encoder (`opensearch-neural-sparse-encoding-doc-v3-distill`, 268 MB, fetched with
`scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json`) into
weighted vocabulary terms that BM25 scores beside the text. Queries need no model: the index
carries the query side, so opening and searching a sparse index takes nothing new.

```python
config = IndexConfig(fields=[...], dense_fields=["contents"],
                     sparse=xtriever.SparseOptionConfig(encoder_dir=sparse_encoder_dir))
index = xtriever.IndexHandle.create("path/to/new-index", config, embedder_dir, reranker_dir, xtriever.LoadPath.MMAP)
index.info().sparse          # SparseInfo(scale=10, boost=1.0, encoder="opensearch-project/…")
index.info().format_version  # 3 — an engine that cannot search the expansion refuses it
```

**Build a sparse index in one go**, adding everything through the handle `create` returned
(`add` or `add_embedded`, which both expand each passage): a sparse index opened later is
searched without the encoder and cannot be added to.

**Use it for corpora whose questions are worded unlike their answers** (FiQA-shaped: no titles)
**and search it with the re-ranker**. Re-ranked, FiQA gained +0.017 nDCG@10 while SciFact and
NFCorpus held within 0.005; without re-ranking SciFact and NFCorpus lost 0.006–0.007 (ADR-0017).
Encoding runs on the build host at a few passages per second on CPU; the lexical index grows
about 2.5–3.9×. `scale` and `boost` default to the engine's (currently 10 and 1.0). A query the index's
query side cannot tokenise is searched on the text fields alone and reported in
`response.stages.sparse_skipped` (`strict=True` raises instead).

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
