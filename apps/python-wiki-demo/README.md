# Xtriever Wikipedia demo (Python)

The second demo of the engine over the same corpus as the iOS one (`apps/ios-wiki-demo`):
a command line that searches all of Simple English Wikipedia through the `xtriever` Python
package and shows the pipeline working — the fused (lexical + dense) list first, then the
re-ranked order with what moved, each hit's eight explained features under the engine's
names, the engine's stage report, and the corpus's identity and licence attribution. It
also builds such an index from the raw snapshot with nothing but the package (`build`), and
checks itself against the goldens the phone is checked against
(`measure`). Feature 019 (`specs/019-python-wiki-demo/`).

The demo owns no retrieval logic: every number it prints is the engine's or a wall clock
around one engine call.

Looking for the recipe before the measurements? `apps/python-minimal-demo/` is the smallest
demo: ten documents in one file, build and search, under 80 lines, no snapshot (Feature 020).

## Inputs

| Input | Default | Override | Produced by |
|---|---|---|---|
| the `xtriever` wheel | — | — | `cd python && .venv/bin/maturin build --release` (→ `target/wheels/`) |
| the embedder | `reference/models/all-MiniLM-L6-v2` | `--embedder`, `XTRIEVER_MODEL_DIR` | `scripts/fetch-model.sh` |
| the re-ranker | `reference/models/ms-marco-MiniLM-L-6-v2` | `--reranker`, `XTRIEVER_RERANK_MODEL_DIR` | `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json` |
| the Wikipedia artefact (`index/`, `ATTRIBUTION.txt`, `expected.json`) | `target/xt-wiki` | `--artefact`, `XTRIEVER_WIKI_ARTEFACT` | `cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki` (hours) or `wikidemo build --limit N --out DIR` (minutes) |
| the snapshot (for `build`) | `reference/datasets/wiki/simple.jsonl` | `--snapshot` | `scripts/fetch-wiki.sh` |
| the chonky splitter model (for `build --chunker chonky` only) | `reference/models/chonky_distilbert_base_uncased_1` | `--chonky`, `XTRIEVER_CHONKY_MODEL_DIR` | `scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json` |

Relative paths resolve against the repository root; a missing input is named with its
producer before anything loads, exit 1.

## Install

```bash
cd apps/python-wiki-demo
uv venv .venv --python 3.12
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test]"          # search, about, measure, build
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test,chonky]"   # …plus build --chunker chonky
.venv/bin/wikidemo --help
```

The demo's own dependencies are the wheel and `tokenizers==0.23.2` (the engine's tokenizer:
it prices the contract chunker's units and counts passages against the window). The
`chonky` extra — `chonky` with `transformers` and `torch`, the heavy install — is only for
`build --chunker chonky`; nothing else imports it, and a chonky build without it is refused
with the install command. The tests need `pytest`.

## In run order

```bash
scripts/fetch-model.sh && scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json   # the two engine models
scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json                            # optional: the chonky splitter, for build --chunker chonky
scripts/fetch-wiki.sh                                                                             # the snapshot (once)
(cd python && .venv/bin/maturin build --release)                                                  # the wheel
cd apps/python-wiki-demo && uv venv .venv --python 3.12 && uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test]" && cd ../..
apps/python-wiki-demo/.venv/bin/wikidemo build --limit 2000 --out target/xt-wiki-slice-py          # a slice: the first 2,000 articles, ~17 min, the shipped recipe
apps/python-wiki-demo/.venv/bin/wikidemo search --artefact target/xt-wiki-slice-py "April"
apps/python-wiki-demo/.venv/bin/wikidemo search --artefact target/xt-wiki-slice-py --explain "April"
apps/python-wiki-demo/.venv/bin/wikidemo about --artefact target/xt-wiki-slice-py
apps/python-wiki-demo/.venv/bin/wikidemo measure                                                   # the shipped index vs the host goldens
```

`--artefact` defaults to the shipped `target/xt-wiki` (built by the Rust CLI, hours); a
demo-built slice is a complete artefact in the same shape. The sections below follow the same
order.

## Build an index

```bash
wikidemo build --limit 2000 --out target/xt-wiki-slice-py                    # the first 2,000 articles, minutes
wikidemo search --artefact target/xt-wiki-slice-py "April"
wikidemo build --chunker chonky --limit 2000 --out target/xt-wiki-slice-chonky   # the same, cut by the chonky splitter
wikidemo build --out target/xt-wiki-py                                        # the whole corpus: 427,947 passages ≈ 11 h on a laptop
```

The recipe, module by module:

1. **verify** (`build.py`): the snapshot's bytes and sha256 against
   `reference/datasets/wiki-manifest.json` before a line is read — a mismatch names both
   hashes and `scripts/fetch-wiki.sh`;
2. **exclude** (`rules.py`): the manifest's rules in order, first match wins —
   `" (disambiguation)"` titles, "may refer to" / "may mean" within the first 300 characters;
3. **split** (`chunking.py` chooses; `--chunker`, Feature 023):
   - **`contract`, the default** (`contract.py`): the Feature 008 contract chunker — the
     recipe the Rust CLI cuts the shipped index with. Paragraphs, then sentences, words and
     byte-halved fragments where a unit does not fit, packed into passages priced by the
     embedder's own tokenizer (`token_count(unit) − 2` against `256 − token_count(title)`), so
     `[CLS] title body [SEP]` always fits the 256-position window. The module is the
     contract's reference implementation (`reference/gen_008_fixtures.py`), replayed byte for
     byte against the 008 fixtures in the tests; the passages are the Rust build's, passage
     for passage;
   - **`chonky`** (`chonky_chunker.py`, Feature 021): each article through the
     [chonky](https://github.com/mirth/chonky) splitter — a small fine-tuned model that
     returns the text as contiguous slices at predicted paragraph breaks; every non-empty
     slice is one passage with its byte range, and the build checks that the slices
     partition the text. Needs the `chonky` extra (`-e ".[chonky]"`) and the model
     (`mirth/chonky_distilbert_base_uncased_1`, pinned by revision and file hashes in
     `reference/models/manifest-chonky.json`, fetched by the same script as the engine's
     models, loaded from disk). The splitter has no length bound: chunks longer than the
     embedder's 256-token window are embedded from their first 256 word-pieces and indexed
     whole for lexical search — 9.2 % on the 2,000-article slice (median 82 tokens, p90 239;
     `specs/021-chonky-wiki-chunking/runs/`); the count is in `about` and in the build's
     record. Feature 022 measured chunking on the BEIR sets and found it corpus-dependent
     (`specs/022-chunking-study/report.md`), which is why this is an option, not the default;
4. **add** through the package: one `Document` per passage — `external_id = "<article id>#<ordinal>"`,
   fields `title` and `text` (`"<title>\n\n<passage>"`), `ChunkInfo(parent, ordinal, byte_start, byte_end)` —
   in batches of 4,096; the engine embeds each passage;
5. **commit**, **merge** (one lexical segment and one compact dense generation — dead rows
   from replaced or deleted passages gone; `IndexConfig(dense_compact_dead_share=…)` makes a
   commit compact on its own once that share of the vectors is dead);
6. the **sidecars**: `index/corpus.json` (the corpus identity, whose chunker block names
   the chunker — the contract's strings, or chonky's model and revision — `record.py`),
   `ATTRIBUTION.txt`, `wiki-build.json`; then `<out>.partial` is renamed
   to `<out>` — nothing openable exists at `<out>` before the build is complete, and an
   existing `<out>` is refused.

**Schema.** The build keeps the shipped index's schema — `title` boosted 2.0 beside `text`,
the dense field `text` (Feature 008's). Feature 013 measured one joined `contents` field **better** on the BEIR sets
(+5.9 nDCG@10 on SciFact, +1.1 on NFCorpus): for a new corpus of your own, index one text
field and make it the dense field —

```python
IndexConfig(fields=[FieldDef(name="contents", kind=FieldKind.TEXT(analyzer="standard_en"))], dense_fields=["contents"])
```

— and put the title on the first line of `contents` as this corpus does.

**The same index as the Rust build.** A default build cuts articles exactly as
`xtriever wiki build` does, so a demo-built slice has the Rust slice's corpus identity and
the same passages, and `wikidemo measure --artefact target/xt-wiki-slice-py --against
target/xt-wiki-slice-rs` is PASS — identity, counts, documents, and the order at every
depth (`specs/023-optional-chonky-chunker/runs/parity-…`). A `--chunker chonky` build is a
different index — its identity says so — and that comparison (correctly) fails; what is
checked for it is that the split partitions the text, on every build.

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
the footprint. Exit 1 on a parity `FAIL`. `--against DIR` compares two artefacts' live
responses instead — a demo-built slice against the Rust-built slice of the same articles.

The committed record (`MacBookPro18,3`, 10 threads, both models memory-mapped): parity
**PASS**, 800 of 800 hits identical on every score bit; medians fused **246 ms**,
re-ranked at depth 10 **1,005 ms** (total 1,251 ms), re-ranked at depth 20 1,683 ms. The
iPhone 16e (Feature 018, same queries, depth 10): 341.5 / 1,369 / 1,704.5 ms. The laptop's
peak resident size (1,029 MB) includes the memory-mapped 1 GB index; the phone's 600 MB
ceiling is a phone rule, recorded for comparison only.

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
