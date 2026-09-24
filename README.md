# Xtriever

A hybrid retrieval engine for retrieval-augmented generation (RAG), written in Rust and built to
run **on the device**: an iPhone, an Android phone or a laptop searches its own index offline,
with no server and no network.

One query goes through four stages:

```
            ┌─ lexical (BM25, tantivy) ─┐
query ──────┤                           ├─ fusion (reciprocal rank) ─ re-rank (cross-encoder) ─ hits
            └─ dense (MiniLM, int8)  ───┘
```

The same engine is reachable from Rust, Python, Swift and Kotlin. On one machine every surface
returns the same hits with the same scores, bit for bit; on another processor only the
cross-encoder's scores move, in the last bits. Three demo applications search Simple English
Wikipedia (about 240,000 articles, 428,000 passages) through it, on a laptop, an iPhone and
Android.

> **Status: 0.1.0, not published.** Everything builds from this repository; nothing is on
> crates.io, PyPI, Swift Package Index or Maven yet. The fifth stage the design names — a
> learned ranker over the pipeline's features (LTR) — is not built: `crates/xtriever-ltr` is a
> placeholder.

## Contents

- [What it does](#what-it-does)
- [How good it is](#how-good-it-is)
- [How fast and how big, on a phone](#how-fast-and-how-big-on-a-phone)
- [Getting started](#getting-started)
- [The four surfaces](#the-four-surfaces)
- [The demos](#the-demos)
- [Measuring it yourself](#measuring-it-yourself)
- [Repository layout](#repository-layout)
- [How the project is built](#how-the-project-is-built)
- [Licence and attribution](#licence-and-attribution)

## What it does

- **Lexical search**: BM25 over the fields you declare, through
  [tantivy](https://github.com/quickwit-oss/tantivy).
- **Dense search**: [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
  embeddings (384 dimensions), run in-process by [candle](https://github.com/huggingface/candle).
  Vectors are stored as eight-bit integers with one scale per vector, a quarter of their float
  size.
- **Fusion**: reciprocal rank fusion of the two lists (`Σ 1/(60 + rank)`).
- **Re-ranking**: the [ms-marco-MiniLM-L-6-v2](https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2)
  cross-encoder re-scores the head of the fused list (20 candidates by default), and the
  final order blends the fused and cross-encoder scores half and half
  ([ADR-0012](docs/adr/0012-interpolated-rerank-default.md)).
- **Optional sparse expansion**: an index can be built with learned term expansions from
  [opensearch-neural-sparse-encoding-doc-v3-distill](https://huggingface.co/opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill).
  It helps corpora whose questions are worded unlike their answers, and is off by default
  ([ADR-0016](docs/adr/0016-sparse-expansion-field.md), [ADR-0017](docs/adr/0017-sparse-option-without-reranking.md)).

Beyond the ranking itself:

- **It degrades instead of failing.** If the dense stage or the re-ranker errors or runs out of
  its time budget, the search returns the previous stage's results and says so in a stage
  report. A strict mode returns the error instead.
- **It is deterministic.** The same index, query and configuration give the same results, ties
  broken by document order.
- **It explains itself.** Every hit can carry each stage's score and rank.
- **It verifies what it loads.** Both models are pinned by size and SHA-256, the Hugging Face
  revision is part of each model's identity, and an index refuses to open with an embedder
  other than the one that built it.
- **It stays small on a phone.** Indexes and models can be memory-mapped read-only, and an index
  opens in place inside a read-only app bundle.

The models run on the CPU. The two default models are eight-bit GGUF artefacts
([leliuga/all-MiniLM-L6-v2-GGUF](https://huggingface.co/leliuga/all-MiniLM-L6-v2-GGUF),
[cstr/ms-marco-MiniLM-L-6-v2-GGUF](https://huggingface.co/cstr/ms-marco-MiniLM-L-6-v2-GGUF)),
51.5 MB together. The float originals load through the same call.

## How good it is

nDCG@10 on the test sets of three [BEIR](https://github.com/beir-cellar/beir) datasets, from
the repository's own evaluation harness with the default models:

| configuration | SciFact | NFCorpus | FiQA | mean |
|---|---|---|---|---|
| BM25 alone | 0.686 | 0.323 | 0.250 | 0.420 |
| dense alone | 0.646 | 0.315 | 0.369 | 0.444 |
| BM25 + dense, fused | 0.715 | 0.354 | 0.370 | 0.480 |
| **fused + re-ranked (the default)** | **0.722** | **0.362** | **0.390** | **0.491** |
| the same with sparse expansion | 0.722 | 0.358 | 0.407 | 0.495 |

Recall@100 of the default pipeline: 0.955, 0.321 and 0.706.

The harness names the rows `lexical-baseline-v2`, `dense-baseline-v1`, `hybrid-baseline-v2`,
`hybrid-rerank-v3` and `hybrid-sparse-rerank-v1`. The records behind every number are committed
under `specs/*/`: [013](specs/013-lexical-quality/) for BM25,
[026](specs/026-eight-bit-precision/report.md) and [027](specs/027-sparse-lexical-expansion/report.md)
for the rest. A ranking change is measured on all three datasets before it lands, and CI runs a
SciFact smoke test of the lexical stage whenever a ranking crate changes.

## How fast and how big, on a phone

The iOS demo searching all of Simple English Wikipedia on an **iPhone 16e**, in Release, over
20 measurement queries, re-ranking 10 candidates
([record](specs/026-eight-bit-precision/runs/iPhone17,5-20260924T053922-mmap-threadsdefault.json)):

| | |
|---|---|
| index | 585 MB for 427,947 passages, opened in place from the app bundle |
| models | 51.5 MB |
| peak memory (`phys_footprint`) | 335 MB, under the project's 600 MB ceiling ([ADR-0010](docs/adr/0010-device-rss-ceiling-600mb.md)) |
| first list (fused), median | 0.20 s |
| re-ranked list, median | 1.21 s more |
| index open | 0.74 s |

The same queries on a 2021 MacBook Pro (M1 Pro), through Python: 0.14 s fused, 0.99 s
re-ranked ([record](specs/026-eight-bit-precision/runs/measure-MacBookPro18,3-20260923T014434Z-mmap-threadsdefault-idle.json)).

Most of a search is the cross-encoder: each re-ranked candidate is one forward pass, whatever
the corpus size. That is why the demos show the fused list first and the re-ranked order after.
A 20,000-passage slice of Wikipedia opens faster on the same phone (0.45 s) and fuses faster
(0.16 s), but re-ranks no faster (the [026 report](specs/026-eight-bit-precision/report.md) has
both).

The Android demo has only emulator records so far. Its results match the host's to the bit
for everything but the cross-encoder, whose scores are within 7e-6 of the host's.

## Getting started

### Prerequisites

- **Rust** through [rustup](https://rustup.rs). `rust-toolchain.toml` pins the toolchain
  (1.91.1, edition 2024) and its targets; `rustup` installs them on first use.
- [cargo-nextest](https://nexte.st) for the tests, and
  [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) for the dependency checks.
- **Python 3.9 or later** with [uv](https://github.com/astral-sh/uv), for the Python package.
  The fixture generators under `reference/` run in pinned environments that
  `scripts/setup-reference-venv.sh <feature>` creates.
- For iOS: Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen). For Android: the SDK,
  the NDK and [cargo-ndk](https://github.com/bbqsrc/cargo-ndk), described in
  [android/xtriever](android/xtriever/README.md).

`scripts/check-toolchain.sh` checks the Rust toolchain and the iOS side.

### Fetch the models

The models are never committed. The fetch script downloads each pinned revision and checks
every file's size and SHA-256:

```bash
scripts/fetch-model.sh --manifest reference/models/manifest-q8.json          # embedder  → reference/models/all-MiniLM-L6-v2-q8
scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json   # re-ranker → reference/models/ms-marco-MiniLM-L-6-v2-q8
```

An eight-bit artefact carries weights only; the script also fetches the float model's
configuration and tokenizer, verified against their own pins.

### Build and test

```bash
cargo build --workspace
cargo nextest run --workspace
```

Tests that need the models or a dataset are marked ignored; run them with
`--run-ignored all` once the models are fetched.

### A first search, in Python

Build the wheel and run the smallest demo: ten documents, one index, one search, first fused,
then re-ranked.

```bash
cd python && uv venv .venv && uv pip install "maturin>=1.15,<2"
.venv/bin/maturin build --release && uv pip install ../target/wheels/xtriever-*.whl
cd .. && python/.venv/bin/python apps/python-minimal-demo/demo.py "how do bees make honey"
```

## The four surfaces

| surface | where | what it is |
|---|---|---|
| Rust | [`crates/xtriever-pipeline`](crates/xtriever-pipeline/src/lib.rs) | `HybridIndex`: create, add, commit, merge, open, search |
| Python | [`python/`](python/README.md) | a wheel built with maturin over the FFI crate |
| Swift | [`swift/Xtriever`](swift/Xtriever/README.md) | a Swift package: an XCFramework and an async `XtrieverIndex` |
| Kotlin | [`android/xtriever`](android/xtriever/README.md) | a Gradle library module for 64-bit ARM Android 8.0 or later |

Python, Swift and Kotlin are generated by [uniffi](https://github.com/mozilla/uniffi-rs) from
one crate, [`crates/xtriever-ffi`](crates/xtriever-ffi), so they expose the same operations and
records. None of them holds retrieval logic of its own.

In Python:

```python
import xtriever

index = xtriever.IndexHandle.open(
    "path/to/index",                                   # a hybrid index directory
    "reference/models/all-MiniLM-L6-v2-q8",            # the embedder (required)
    "reference/models/ms-marco-MiniLM-L-6-v2-q8",      # the re-ranker (or None: fused order only)
    xtriever.LoadPath.MMAP,                            # both models memory-mapped
)
response = index.search("why is the sky blue", xtriever.SearchOptions(k=5, explain=True))
for hit in response.hits:
    print(hit.external_id, hit.score, hit.rerank_score, hit.text[:80])
```

[python/README.md](python/README.md) covers building an index, documents and fields, errors and
the sparse option. The Swift and Kotlin READMEs show the same in their languages.

## The demos

| demo | what it shows |
|---|---|
| [`apps/python-minimal-demo`](apps/python-minimal-demo/README.md) | the whole pipeline in one file under 80 lines, over ten documents |
| [`apps/python-wiki-demo`](apps/python-wiki-demo/README.md) | a command line over Simple English Wikipedia: search, explanations, stage reports, building the index from the raw snapshot, and a parity check against the phone's goldens |
| [`apps/ios-wiki-demo`](apps/ios-wiki-demo/README.md) | a SwiftUI app searching Wikipedia offline on an iPhone: the whole edition, or a 20,000-passage slice for a quicker install |
| [`apps/android-wiki-demo`](apps/android-wiki-demo/README.md) | the same as a Jetpack Compose app on Android |

The Wikipedia index is built by the command line from a pinned snapshot:

```bash
scripts/setup-reference-venv.sh 008                                # the converter's pinned Python environment
scripts/fetch-wiki.sh                                              # the pinned snapshot, verified
RAYON_NUM_THREADS=1 cargo run --release -p xtriever-cli -- wiki build \
  --out target/xt-wiki --cache-dir target/xt-wiki-cache            # about 12 hours; resumable
```

Add `--limit N` to build from the first N articles. The iOS README gives the slice's build.

## Measuring it yourself

The evaluation harness downloads the BEIR datasets, verifies them by hash, and scores a
configuration:

```bash
scripts/fetch-beir.sh scifact                     # or nfcorpus, fiqa; all three by default
cargo run --release -p xtriever-eval --example beir -- run --dataset scifact \
  --config hybrid-rerank-v3 --out /tmp/scifact.json
cargo run --release -p xtriever-eval --example beir -- delta \
  specs/026-eight-bit-precision/runs/hybrid-rerank-v3.scifact.json /tmp/scifact.json
```

The first run embeds the corpus and caches it (minutes for SciFact, over an hour for FiQA).
`--config` takes the names under [How good it is](#how-good-it-is), and `delta` prints the change
against any committed record.

Behaviour that has a reference implementation (BM25 scoring, tokenization, embeddings, the
cross-encoder, the sparse encoder) is tested against golden files generated by the Python
scripts in [`reference/`](reference), at tolerances each feature's spec states.

## Repository layout

```
crates/
  xtriever-core       the contract: traits, types, errors (std only)
  xtriever-analysis   text analysis and chunking (std only)
  xtriever-lexical    BM25 through tantivy
  xtriever-dense      the embedder, int8 vectors, the sparse encoder
  xtriever-rerank     the cross-encoder
  xtriever-ltr        placeholder for the learned ranker (not built)
  xtriever-pipeline   the hybrid index: fusion, re-ranking, degradation (std only)
  xtriever-eval       the BEIR evaluation harness (std only)
  xtriever-ffi        the uniffi surface for Python, Swift and Kotlin
  xtriever-cli        `xtriever wiki build | verify | expected`
python/               the Python package
swift/Xtriever/       the Swift package
android/xtriever/     the Kotlin library module
apps/                 the four demos
reference/            Python reference implementations and fixture generators
scripts/              model, dataset and platform build scripts, the demo check
specs/NNN-*/          one directory per feature: spec, plan, tasks, report, run records
docs/adr/             architecture decision records
```

Dependencies point one way: `core` ← stage crates ← `pipeline` ← `ffi` and `cli`. Crates
marked "std only" have no C or C++ dependencies, no async and no threads of their own. The C
and C++ dependencies stay in `dense`, `rerank` and `ffi`, and `deny.toml` enforces that.

## How the project is built

[`.specify/memory/constitution.md`](.specify/memory/constitution.md) is the project's highest
authority. Its rules:

- the tests come first, and they are committed failing;
- nothing is claimed without a measurement;
- a ranking change that lowers quality needs a recorded decision;
- the pure crates stay pure;
- library code never panics, and every C/C++ dependency is licence-checked and banned outside
  its crates.

Each feature goes through [Spec Kit](https://github.com/github/spec-kit) — specify, clarify,
plan, tasks, implement — and leaves a directory under `specs/` with its spec, plan, task list,
run records and a report stating what was measured, what was found and what was deliberately
not done. There are 27 so far; the reports are the best history of why the engine is the way
it is. Decisions that change a contract, a format or the rules are recorded in
[`docs/adr/`](docs/adr).

Before a change is done, the local gate runs formatting, clippy with warnings as errors, the
test suite, the dependency checks, the iOS, Android and wasm32 builds (wasm32 fails today, a
known and tracked gap), the reference fixtures, the Python package, and
`scripts/check-demos.sh`, which builds every demo against the engine. The full list is in
[`CLAUDE.md`](CLAUDE.md). CI runs the portable part on Linux, macOS and Windows.

## Licence and attribution

Xtriever is licensed under the [Apache License, Version 2.0](LICENSE).

The models are not part of this repository and carry their own licences; see each model's card
on Hugging Face (linked above). The Wikipedia text the demos search is from
[Simple English Wikipedia](https://simple.wikipedia.org), available under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/); every Wikipedia index built
here ships its attribution file, and every Wikipedia demo shows it. The BEIR datasets are fetched from
their published sources and are subject to their own terms.
