# Xtriever Wikipedia demo (Python)

The second demo of the engine over the same corpus as the iOS one (`apps/ios-wiki-demo`):
a command line that searches all of Simple English Wikipedia through the `xtriever` Python
package and shows the pipeline working — the fused (lexical + dense) list first, then the
re-ranked order with what moved, each hit's eight explained features under the engine's
names, the engine's stage report, and the corpus's identity and licence attribution. It
also builds such an index from the raw snapshot with nothing but the package (`build`,
Feature 019 PR B), and checks itself against the goldens the phone is checked against
(`measure`). Feature 019 (`specs/019-python-wiki-demo/`).

The demo owns no retrieval logic: every number it prints is the engine's or a wall clock
around one engine call.

## Inputs

| Input | Default | Override | Produced by |
|---|---|---|---|
| the `xtriever` wheel | — | — | `cd python && .venv/bin/maturin build --release` (→ `target/wheels/`) |
| the embedder | `reference/models/all-MiniLM-L6-v2` | `--embedder`, `XTRIEVER_MODEL_DIR` | `scripts/fetch-model.sh` |
| the re-ranker | `reference/models/ms-marco-MiniLM-L-6-v2` | `--reranker`, `XTRIEVER_RERANK_MODEL_DIR` | `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json` |
| the Wikipedia artefact (`index/`, `ATTRIBUTION.txt`, `expected.json`) | `target/xt-wiki` | `--artefact`, `XTRIEVER_WIKI_ARTEFACT` | `cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki` (hours) or `wikidemo build --limit N --out DIR` (minutes) |
| the snapshot (for `build`) | `reference/datasets/wiki/simple.jsonl` | `--snapshot` | `scripts/fetch-wiki.sh` |

Relative paths resolve against the repository root; a missing input is named with its
producer before anything loads, exit 1.

## Install

```bash
cd apps/python-wiki-demo
uv venv .venv --python 3.12
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test]"
.venv/bin/wikidemo --help
```

The demo's own dependencies are the wheel and `tokenizers==0.23.2` (the version the engine
pins, used only by `build` to price passages); the tests need `pytest`.

## Search

```bash
wikidemo search "why is the sky blue"
wikidemo search --explain --depth 20 "who painted the Mona Lisa"
wikidemo search --depth 0 --snippet 80 "how do vaccines work"
wikidemo search --budget-ms 300 "what is the capital of Australia"            # a stage cut short shows in the report
wikidemo search --budget-ms 300 --strict "what is the capital of Australia"   # …or as the engine's error, exit 1
wikidemo search --mode replace "who painted the Mona Lisa"                    # the pre-015 re-rank order
```

| Flag | Default | What it does |
|---|---|---|
| `-k N` | 10 | hits to print |
| `--depth {0,5,10,20}` | **10** | how many fused candidates the cross-encoder re-ranks. The engine's own default is 20; Feature 014 measured depth 10 at −0.3 mean nDCG@10 on the BEIR sets (0.4881 vs 0.4913) for half the cross-encoder calls, and Feature 017 measured the reference phone at 1.41 s instead of 2.31 s per re-ranked search — so both demos default to 10 (Feature 018). `0` prints the fused list only. |
| `--budget-ms MS` | none | a time budget; a stage that runs out degrades to the previous stage's result and the report says so |
| `--strict` | off | the budget raises the engine's error instead |
| `--mode {interpolate,replace}` | interpolate | the re-ranked order: `0.5·minmax(fused) + 0.5·minmax(cross-encoder)` (the engine's default since Feature 015, ADR-0012) or the previous replace order — asked for explicitly either way, whatever the index recorded |
| `--explain` | off | the eight pipeline features under each hit (`bm25.score`, `bm25.rank`, `dense.score`, `dense.rank`, `fused.score`, `rerank.score`, `rerank.rank`, `rerank.combined`); "not seen by this stage" where a stage did not retrieve the hit |
| `--snippet N` | whole passages | cut passages to N characters |

A search is two engine calls, as in the iOS demo: the fused stage (depth 0) is printed as
soon as it returns, then the re-ranked stage with a mark per hit — `↑n` / `↓n` / `=` / `new`
and the fused hits that dropped out of the head. Then the engine's stage report (candidate
counts, degradation, the re-rank report, the time-limit flag, elapsed ms) and the wall
clocks with the process's peak resident size. Every process's first search is its warm-up
(the vectors page in).

## About

```bash
wikidemo about
```

The corpus (edition, snapshot date, counts, identity — from the index's `corpus.json`),
the engine's index information (`info()`: documents, format version, model fingerprints,
depths, the re-rank mode the index recorded), the engine's re-rank depth beside the demo's default,
this session's open and load times, and `ATTRIBUTION.txt` verbatim with the licence link.

## Measure

```bash
wikidemo measure                      # the shipped index against the host goldens (target/xt-wiki/expected.json)
```

Runs the twenty measurement queries (`reference/fixtures/008/queries.json`) at re-rank
depths 0 / 5 / 10 / 20 and compares every hit with the host goldens by the device test's
rule (lexical bits exact, fused order identical, dense / re-rank scores within 1e-3 per
document, every hit present) — the same oracle the iPhone run is checked against — then
writes a record under `specs/019-python-wiki-demo/runs/` with the per-depth latency and
the footprint. Exit 1 on a parity `FAIL`.

The committed record (`MacBookPro18,3`, 10 threads, both models memory-mapped): parity
**PASS**, 800 of 800 hits identical on every score bit; medians fused **246 ms**,
re-ranked at depth 10 **1,005 ms** (total 1,251 ms), re-ranked at depth 20 1,683 ms. The
iPhone 16e (Feature 018, same queries, depth 10): 341.5 / 1,369 / 1,704.5 ms. The laptop's
peak resident size (1,029 MB) includes the memory-mapped 1 GB index; the phone's 600 MB
ceiling is a phone rule, recorded for comparison only.

## Build

*Lands with Feature 019's second PR:* `wikidemo build --limit N --out DIR` — the raw
snapshot to a searchable index with the Feature 008 recipe through the package alone, and
`wikidemo measure --artefact DIR --against DIR2` to check a demo-built slice against the
Rust build of the same slice (ids in the same order at every depth, identity and counts
equal). Until then `build` is not offered.

## Threads

The engine's CPU pool follows `RAYON_NUM_THREADS`; set it before the process starts
(`RAYON_NUM_THREADS=1 wikidemo search …`). Records name the effective count and its source.

## Tests

```bash
.venv/bin/pytest tests -q                 # everything, with the models and the 007 fixture index on disk
.venv/bin/pytest tests -q -m "not models" # the model-free subset (rendering, URL cases, marks, identity, inputs, the comparison rule)
```

The demo's own logic is checked against the 40-document fixture and its goldens
(`swift/Xtriever/Tests/Fixtures/`) — no 1 GB artefact needed — and the search output
against the engine's score bits.
