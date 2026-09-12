# Phase 0 Research: The Evaluation Harness

**Feature**: `003-eval-harness` | **Date**: 2026-09-12 | **Plan**: [plan.md](./plan.md)

Everything below that could be measured was measured on 2026-09-12 rather than recalled: the
three archives were downloaded and hashed, their files counted and inspected, `pytrec_eval`'s
conventions were probed with a synthetic run, BEIR's wrapper was read from its repository, and the
published figures were read from the paper's own text (Agent Operating Rule 1; Principle IV).

---

## D1. Datasets are pinned by measured hashes; content is fetched by a script, never committed

**Decision**: `scripts/fetch-beir.sh [dataset…]` downloads each archive with `curl` into
`reference/datasets/beir/` (git-ignored), verifies the archive's SHA-256 and size against the
committed manifest, extracts it with `unzip`, and verifies every file the harness reads. The Rust
loader re-verifies each file's hash on every load before parsing (FR-006/FR-007).

**Measured pins** (source `https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/<name>.zip`):

| dataset | archive bytes | archive SHA-256 | server `Last-Modified` |
|---|---|---|---|
| `scifact` | 2,816,079 | `536e14446a0ba56ed1398ab1055f39fe852686ecad24a6306c80c490fa8e0165` | 2021-04-20 |
| `nfcorpus` | 2,448,432 | `efe5be03f8c5b86a5870102d0599d227c8c6e2484328e68c6522560385671b0b` | 2021-01-15 |
| `fiqa` | 17,948,027 | `32c7df99ed21252fdfb2cf3f5673502a8d245ee0c44c4a133570d92ce2b3ad02` | 2021-03-02 |

| file | bytes | lines | SHA-256 |
|---|---|---|---|
| `scifact/corpus.jsonl` | 8,106,566 | 5,183 | `dec31c8182f3d744c7d2c09423756fd1d17cbef75808db13ba01cc0aab4d1ac6` |
| `scifact/queries.jsonl` | 209,731 | 1,109 | `8ff84a7c903f722981cd8d595c022660140c51867b27608a6d4910db86080313` |
| `scifact/qrels/test.tsv` | 5,389 | 340 (1 header + 339) | `0864bb985e0ca2367ba217977e72004d549054b2b06666ed9d4825ac7c21284c` |
| `nfcorpus/corpus.jsonl` | 6,219,364 | 3,633 | `10cc83ef1826b1425e6a87090b5140b39b27755d5a27e48215a88611c899991f` |
| `nfcorpus/queries.jsonl` | 441,466 | 3,237 | `d024e6621b84925d485ae473d316a0c3af31c62c8068a59fb29d22f7613aef2a` |
| `nfcorpus/qrels/test.tsv` | 279,572 | 12,335 (1 + 12,334) | `f8fba6ef3d4dd9c3a242a8ba4ae38276fc3622fce7dcbae764766d564542fd2a` |
| `fiqa/corpus.jsonl` | 47,949,497 | 57,638 | `ff593e4df9933955dc3af83be0c3fa28ac7465f627e08c2e53593e734d506517` |
| `fiqa/queries.jsonl` | 706,247 | 6,648 | `eede1e61d4a0188940239b53ebc2da91f577a6a34679c812d1eb9090c29877bc` |
| `fiqa/qrels/test.tsv` | 25,256 | 1,707 (1 + 1,706) | `6adc2a640dcdd22bb8b3858f89107adef2a7c3db20a63550dfa7a0f71e379e44` |

**Counts to assert at load** (FR-009; replaces the spec's "≈" figures):

| dataset | documents | queries in `queries.jsonl` | **test** queries judged | judgement pairs | grades present |
|---|---|---|---|---|---|
| SciFact | 5,183 | 1,109 | **300** | 339 | 1 |
| NFCorpus | 3,633 | 3,237 | **323** | 12,334 | 1, 2 |
| FiQA-2018 | 57,638 | 6,648 | **648** | 1,706 | 1 |

Referential integrity, measured: every judged query is in `queries.jsonl` and every judged
document is in the corpus for all three (0 dangling references) — FR-010's reporting path exists
but will report zero. **FiQA has 55 query ids that equal a document id** (both are small integers
as strings); that matters under D3. **FiQA titles are all empty**; SciFact and NFCorpus titles are
all non-empty.

**Rationale**: `curl` + `shasum` + `unzip` are on every CI runner and the development host; a
Rust HTTP client would add dependencies to a crate the constitution keeps `std`-only for no
benefit. Hashes identify content independently of the URL, so a moved mirror is harmless and a
changed file is a loud failure (Principle IV).

**Alternatives considered**: a Rust downloader in an example binary (`ureq`) — more code, no
gain. Committing SciFact (2.8 MB) — violates FR-008 and the datasets' own distribution terms;
rejected. Using the `beir` Python package's downloader — pulls torch etc. into the venv for a
`curl`; rejected.

## D2. File formats (inspected)

- `corpus.jsonl`: one JSON object per line, keys `_id`, `title`, `text`, `metadata`. Empty
  `title` is the empty string, not absent.
- `queries.jsonl`: `_id`, `text`, `metadata`.
- `qrels/test.tsv`: **a header line `query-id<TAB>corpus-id<TAB>score` followed by data rows,
  with CRLF line endings** in all three datasets. The loader must strip `\r` and skip the header;
  a naive `lines()` + `split('\t')` would carry `"1\r"` as a grade. Grades are integers ≥ 1 in
  these three (0 never appears here, but D3 handles it).
- Only `qrels/test.tsv` is read (FR-011); `train`/`dev` are ignored and not hashed.

## D3. Metric conventions, measured against `pytrec_eval 0.5` and BEIR's wrapper

Probe run (`target/pte-venv`, `pytrec_eval 0.5`, `numpy 2.5.3`, Python 3.12), synthetic qrels and
run covering every FR-002 edge case. Results, and therefore the harness's rules:

| situation | `pytrec_eval` | harness rule |
|---|---|---|
| nDCG gain | **linear**: `rel / log2(rank+1)`; hand-computed 0.8597186998521972 matched exactly; exponential gain would have given 0.7967 | linear gain, natural `log2(rank+1)` discount, ideal DCG from all grades > 0 sorted descending, cut at 10 |
| Recall@100 | `|relevant ∩ top100| / |relevant|`, relevant = grade > 0 | same |
| query in run, **not in qrels** | absent from output | ignored, counted in the report as "unjudged" |
| query in qrels, **not in run** | absent from output (BEIR's mean is over `scores.keys()`) | excluded from the mean, counted as "not retrieved"; the harness always submits every judged query, so this count is 0 for `lexical-baseline-v1` |
| judged query with **no relevant document** (all grades 0) | returned with 0.0 | counted as 0.0 in the mean |
| judged query with an **empty** result list | returned with 0.0 | counted as 0.0 |
| grade 0 in qrels | non-relevant; not in the recall denominator | same |
| **ties** in the run's scores | trec_eval orders equal scores by **document id descending** (measured: `d9, d2, d1`) | the harness scores the retriever's **ordered list**, never re-sorts; see D4 for how the oracle is fed |
| identical query id and document id | BEIR's `evaluate(…, ignore_identical_ids=True)` **pops** such results before scoring (`beir/retrieval/evaluation.py`, read 2026-09-12) | replicated: dropped before scoring, counted in the report — FiQA has 55 such id collisions, so a retrieved self-id would otherwise silently differ from BEIR |
| mean | BEIR sums per-query values over `scores.keys()` and `round(…/len, 5)` | full-precision mean in a **fixed order** (queries sorted by id), reported alongside the 5-decimal BEIR-style rounding |

**Rationale**: these are the conventions behind every published BEIR number; matching them is
what makes the FR-020 band meaningful and the goldens honest. Each row above becomes at least one
golden case (FR-002).

## D4. The oracle is fed rank-based scores so it scores the same order the harness scores

**Decision**: `reference/gen_003_fixtures.py` builds each synthetic run as an ordered list and
hands `pytrec_eval` scores `k − rank` (strictly decreasing), so trec_eval's docid-descending
tie rule can never reorder it. The Rust metric layer takes `&[&str]` ordered ids per query. For the
real baseline, the Rust runner exports its ordered results to a run file and the Python script
scores that file the same way (`--verify-run`), which cross-checks the *real* baseline numbers
against the reference (SC-001 at scale, not only on synthetic data).

**Why**: the lexical stage's order is already deterministic (Feature 002, ties by ascending
`DocId` — note: *internal* id, not the BEIR string id); the metric must evaluate exactly that
order. Feeding raw BM25 scores to trec_eval would let it re-break ties by external id descending
and the two implementations would legitimately disagree.

## D5. `lexical-baseline-v1`, concretely

| item | value |
|---|---|
| schema | `title`: `Text("standard_en")`, indexed, not stored, boost **2.0**; `text`: `Text("standard_en")`, indexed, not stored, boost 1.0 |
| empty title | field omitted from the document (BEIR indexes "the title (if available)"); all of FiQA |
| ids | corpus order ⇒ dense `DocId(0..n)`; the harness keeps `Vec<String>` for the reverse map; the backend never sees the string id (Principle V, FR-014) |
| query | `LexicalQuery::Match(None, query_text)` — analyzed per field, OR-ed, summed with field boosts (002 FR-017) |
| `k` | 100 |
| batches | one `add` of the whole corpus, one `commit` — the layout the 002 goldens were minted from; FiQA's 57,638 documents in one batch is ~48 MB of text, well within the writer arena's flushing behaviour |

Published comparison point (FR-020), **pinned from the BEIR paper**, arXiv 2104.08663, Table 2
(nDCG@10) and Table 9 (Recall@100), BM25 column — text extracted from the PDF on 2026-09-12:

| dataset | published BM25 nDCG@10 | published BM25 Recall@100 | FR-020 band (nDCG@10) |
|---|---|---|---|
| SciFact | **0.665** | 0.908 | 0.565 – 0.765 |
| NFCorpus | **0.325** | 0.250 | 0.225 – 0.425 |
| FiQA-2018 | **0.236** | 0.539 | 0.136 – 0.336 |

The paper's BM25 is *"Anserini with the default Lucene parameters (k=0.9 and b=0.4). We index
the title (if available) and passage as separate fields"* (§3, verbatim). tantivy's constants are
k1 = 1.2, b = 0.75 and are not configurable (001 D-notes), and the analyzers differ, so the band
is a harness-validity check exactly as the spec says — not a claim that the two agree.

## D6. Crate layout: the library never depends on a stage crate

**Decision**:

- `xtriever-eval` **library** (`std`-only, Principle III): `dataset` (manifest, hash-verified
  loading, CRLF-safe qrels), `metrics` (nDCG@10, Recall@100 over ordered ids), `run` (per-query
  ordered results + the identical-id rule), `report` (JSON report, delta, ADR-trigger line),
  `config` (`EvalConfig` → core `Schema` + `Document`s + `LexicalQuery` builder). Depends on
  `xtriever-core`, `serde`, `serde_json`, `sha2` only.
- `xtriever-eval` **example binary** `beir` (`examples/beir.rs`): `fetch-check`, `run`, `delta`,
  `smoke` subcommands. Constructs `TantivyIndex` through a **dev-dependency** on
  `xtriever-lexical`, so the library's dependency graph stays `eval → core` and the example's is
  `eval → lexical → core` — downward both ways (FR-026). `anyhow` is allowed in a binary.
- Peak memory and index size (FR-018) are measured **outside** the process: `/usr/bin/time -l`
  on macOS (`maximum resident set size`) / `-v` on Linux, and `du -sk` on the index directory. No
  `libc` dependency, no platform code; the method is recorded next to the number.

**Alternatives considered**: putting the runner in `xtriever-cli` — that crate has no spec yet
and would gain an unreviewed dependency on `lexical`; rejected. Reading RSS in-process via `libc`
— an FFI dependency for one number that the shell measures better; rejected.

## D7. Report format and delta

**Decision**: `EvalReport` as pretty JSON with stable key order, committed at
`specs/003-eval-harness/baselines/lexical-baseline-v1.json`, plus the same numbers as a Markdown
table in `report.md`. `beir delta before.json after.json` prints the FR-021 table and the FR-022
ADR-trigger line. `beir smoke --baseline <json>` runs SciFact and applies FR-024 with tolerance 0.

**Determinism**: per-query values summed in query-id order into an `f64`; two runs of the same
inputs are bit-identical (FR-004). The report records lexical commit `94ddbe67f926badf962b93e8bd29d687e300189a`,
the harness commit, the config name, every file hash from the manifest, and the counts.

## D8. CI smoke job

**Decision**: a new `eval-smoke` job in `.github/workflows/ci.yml`, `ubuntu-latest`, triggered
on push/PR like the others but with a step-level `paths` check via `dorny/paths-filter` (or the
`on.paths` equivalent — decided at implementation, cited in the workflow comment) for
`crates/xtriever-{lexical,dense,rerank,ltr,pipeline}/**`. Steps: checkout; `actions/cache` on
`reference/datasets/beir/scifact*` keyed by the manifest's SciFact archive hash; `scripts/fetch-beir.sh scifact`
(no-op on a cache hit, hashes re-verified either way); `cargo run -p xtriever-eval --example beir --
smoke --dataset scifact --baseline specs/003-eval-harness/baselines/lexical-baseline-v1.json`.
Blocking (FR-023). The escape hatch in FR-023 is a `continue-on-error: true` line with a comment
naming the reason and the re-blocking commit — never silent.

## D9. Python reference environment

`reference/requirements-003.in` = `pytrec_eval==0.5`, `numpy` (pinned to the resolved 2.5.3) —
**not** the 001/002 torch stack; a separate, tiny `reference/.venv-003`. `setup-reference-venv.sh 003`
already supports it. `reference/gen_003_fixtures.py` emits `reference/fixtures/003/metrics.json`
(the synthetic goldens) with a manifest, and offers `--verify-run <run.jsonl> --qrels <tsv>` to
score a Rust-exported run with `pytrec_eval` and BEIR's exact wrapper semantics (identical-id pop,
mean over returned keys, 5-decimal rounding).

## D10. Dependencies (added with `cargo add`)

| crate | where | why |
|---|---|---|
| `serde`, `serde_json` | lib | JSONL, manifest, report |
| `sha2` | lib | file verification (pure Rust; already in the workspace via 001) |
| `xtriever-core` | lib | `Schema`, `Document`, `LexicalQuery`, `LexicalIndex`, `Hit` |
| `xtriever-lexical` | **dev** | the example binary builds `TantivyIndex` |
| `anyhow` | dev | example binary error handling (allowed in binaries) |
| `tempfile` | dev | tests |

No `deny.toml` change; purity asserted by `cargo tree -p xtriever-eval -e normal` (the library
graph excludes dev-deps).

---

## Risks

| # | risk | mitigation |
|---|---|---|
| R1 | The BEIR mirror goes away or its archives change | hashes pinned; a change is a loud failure and a new pin + new baseline, never silent |
| R2 | `lexical-baseline-v1` falls outside the ±0.10 band on some dataset | a finding to investigate (analyzer / k1,b / field handling); the band is not widened (Rule 6) |
| R3 | FiQA in one `add` batch stresses the 15 MB writer arena | the backend flushes segments when the arena fills; correctness is unaffected, only segment count — and the tie-break no longer depends on layout (002 F-001) |
| R4 | CI download flakiness | cache keyed by hash; `curl --retry 5` with backoff and a bounded connect timeout (added after the first CI run failed with `curl (7)`); FR-023's one-cycle escape hatch if the host is unreachable from GitHub runners outright. No byte-identical mirror exists: Hugging Face's `BeIR/scifact` holds Parquet conversions |
| R5 | `pytrec_eval` wheel availability on 3.12 | measured today: `pytrec_eval 0.5` installed from source with `numpy 2.5.3` on Python 3.12 without a compiler error |
| R6 | SciFact end-to-end exceeds SC-008's one minute | measured at implementation; if slower it is recorded, not failed |
