# Research: The Python Wikipedia Demo

Every decision below was checked against the pinned code on 2026-09-17; the items cited are
the ones the plan relies on (Agent Operating Rule 1).

## D1 — The interface: one console script, four subcommands

**Decision**: `wikidemo` (also `python -m wikidemo`) with `search`, `about`, `build`,
`measure`; `argparse` from the standard library; plain text to stdout, diagnostics to
stderr; exit 0 on success (an empty result is success), 2 on a usage error, 1 on a
missing input or an engine error. Flags are the settings; nothing is persisted.

**Rationale**: the owner's Q1 — "the simplest textual UI possible, like `demo search
"sky color"`". A subcommand per user story keeps each independently runnable and testable.

**Alternatives**: a REPL (state, but no scriptability); a `--json` mode (deferred — the
records are the machine-readable output, and the engine's Python surface already is the
API).

## D2 — Fused first, then re-ranked: two engine calls

**Decision**: `search` runs `SearchOptions(k, rerank_depth=0, explain=True)` and prints the
fused list, then `SearchOptions(k, rerank_depth=depth, …)` and prints the re-ranked list
with marks. With `--depth 0` the second call is skipped.

**Rationale**: identical to the iOS demo (`Settings.fusedOptions` / `rerankedOptions`,
`apps/ios-wiki-demo/App/Model/Settings.swift`), so the two demos' records compare: the
fused wall time is a depth-0 search, the re-ranked one a depth-N search, the total their
sum. The engine's fused order is deterministic (Principle VI), so the two calls' fused
heads agree — the marks are meaningful.

## D3 — Change marks: the iOS rule, ported

**Decision**: `marks(fused_ids, reranked_ids) -> (dict[id, Mark], dropped_ids)` exactly as
`ChangeMark.compute` (`apps/ios-wiki-demo/App/Model/ChangeMark.swift`): for each re-ranked
hit at rank r, `before = fused rank`; absent → `new`; `delta = before − r`; 0 → `same`,
> 0 → `up(delta)`, < 0 → `down(−delta)`; fused hits not in the re-ranked list → dropped
(printed after the list). Rendered `↑n`, `↓n`, `=`, `new`.

## D4 — Title, passage, URL: the 008 convention, as the Swift package

**Decision**: `title_and_passage(text)` splits at the first `"\n\n"` (`None` when absent —
the fixture index has no title line: the external id stands in for the title, the whole
text is the passage, no link — as `DisplayedHit` does); `wikipedia_url(title)` =
`https://simple.wikipedia.org/wiki/` + the title's UTF-8 bytes with everything outside
`A–Z a–z 0–9 - _ . ~ /` percent-encoded upper-case (`Hit.wikipediaURL(forTitle:)` in
`swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`; `derive_url` in
`crates/xtriever-cli/src/wiki/url.rs`). The test carries the CLI's eleven cases verbatim.
Position in the article = `hit.chunk.ordinal` (`ChunkInfo.ordinal`, `types.rs:85`),
rendered "passage *ordinal + 1* of the article" as the iOS detail view does; the article
id = `hit.chunk.parent`.

**Not** `urllib.parse.quote`: its safe set differs (`~` handling varies by version, `/` is
safe by default but so are other characters in some versions); the explicit byte loop is
eleven lines and provably the CLI's.

## D5 — The chunker: the 008 reference implementation, copied and fixture-tested

**Decision**: `wikidemo/chunking.py` carries the contract implementation from
`reference/gen_008_fixtures.py` (`WHITESPACE`, `trim`, `collapse`, `paragraphs`,
`sentences`, `words`, `fragments`, `chunk`) unchanged in behaviour, with the module's
provenance in its docstring. Its tests replay `reference/fixtures/008/chunk_a.json`
(48 cases, cost = words) and `chunk_b.json` (9 articles, cost from the fixture's
`unit_costs` table — no tokenizer needed in tests) and require byte-identical `text`,
`byte_range` and `cost` — the same comparison the Rust golden test makes.

Pricing at build time: `tokenizers.Tokenizer.from_file(<embedder>/tokenizer.json)`,
`no_truncation()`, `no_padding()`, `cost(unit) = len(tok.encode(unit,
add_special_tokens=True).ids) − 2`; budget `256 − token_count(title)`; a title of ≥ 256
positions is a build error naming the article (as `documents_for`,
`crates/xtriever-cli/src/wiki/chunking.rs:66-79`). `tokenizers==0.23.2` is the version
`crates/xtriever-dense/Cargo.toml:23` pins and `reference/requirements-008.txt` uses; the
008 report records that `MiniLmEmbedder::token_count` matches all 813 priced units.
Unit costs are memoised per build (the reference does the same).

**Rationale**: the demo must show the recipe *in the demo* — a reader should not have to
open `reference/`; and a copy that is fixture-tested cannot drift from the contract without
the test saying so. Importing a script from `reference/` by `sys.path` would tie the demo
to the repository layout in a way an `apps/` program should not be.

**Alternative**: exposing `token_count` and the chunker through the FFI — a package-wire
change the spec forbids (FR-019) and a Rust change the feature does not need.

## D6 — Exclusion rules: ported from `rules.rs`

**Decision**: `excluded_by(rules, title, text) -> index | None`, first match wins, from the
manifest's `exclusions` list: `title_suffix` = `title.endswith(value)`; `lead_contains` =
the phrase's *start* lies before the `within_chars`-th character (character offsets, not
bytes — `text.find(value)` < the byte offset of char `within_chars`, or `len(text)` when
the text is shorter). Counted per rule under the Rust names
(`title_suffix:<value>`, `lead_contains:<value>:<n>`, `build.rs:330-335`).

## D7 — The schema and configuration: the shipped index's

**Decision**: `IndexConfig(fields=[FieldDef("title", TEXT("standard_en"), indexed=True,
stored=False, boost=2.0), FieldDef("text", TEXT("standard_en"), boost=1.0)],
dense_fields=["text"])` with the defaults `candidate_depth=100, rrf_k=60,
rerank_depth=20, rerank_mode=None` (`types.rs:248-266`), which the FFI maps to
`RerankMode::default()` at create (`index.rs:217`) — exactly `wiki_config()`
(`chunking.rs:39-54`) plus `HybridConfig::new`'s defaults (`types.rs:48-57`). The
document: `external_id = f"{id}#{ordinal}"`, fields `title` = the title, `text` =
`f"{title}\n\n{body}"`, `chunk = ChunkInfo(parent=id, ordinal, byte_start, byte_end)`
(`source_document`, `chunking.rs:103-120`).

**Rationale**: the spec's first assumption — the Rust build is the oracle and a full build
matches the phone. The README states Feature 013's finding (one joined `contents` field:
+5.9 SciFact, +1.1 NFCorpus) and shows the one-line change for a new corpus.

## D8 — Embedding through `add`: bit-identical to the Rust build

**Decision**: documents go through `IndexHandle.add(docs)` in batches of 4,096 (the CLI's
`SHARD`), one `commit()` at the end, then `merge()`.

**Rationale**: `HybridIndex::add` embeds each passage alone —
`embed(&[passage], Passage)` (`crates/xtriever-pipeline/src/index.rs:379-395`) — and so does
the CLI build on a cache miss (`build.rs:373`, `embed(&[text], Passage)`); same model, same
engine, same one-passage batch → the same vector bits. The lexical stage assigns `DocId`s
in add order; both builds add in snapshot order. Hence the slice check (D14) expects equal
score bits, not merely equal order — and reports either way. The Rust cache
(`target/xt-wiki-cache`) is not read: its format is the CLI's own and using it would hide
the cost the demo is meant to show.

## D9 — Corpus identity, sidecar, attribution: reproduced

**Decision**: `record.py` computes the identity as `record.rs:106-121` does: sha256 of the
canonical JSON of `{snapshot, exclusions, chunker, embedder_fingerprint[, partial]}` —
`json.dumps(basis, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`, which
`canonical_json`'s doc comment states is byte-identical (`record.rs:124-126`, tested there
against Python). `snapshot` = `{edition, snapshot_date, parquet_sha256, jsonl_sha256}` from
the manifest; `chunker` = `{"version": 1, "budget": "256 - token_count(title)", "cost":
"MiniLmEmbedder::token_count(unit) - 2"}` — the shipped `corpus.json`'s literal strings
(the identity names the contract, not the implementation); `embedder_fingerprint` from
`info().embedder_fingerprint`. Test: the shipped `target/xt-wiki/index/corpus.json`'s
basis hashes to `ea0fc78c…eab2027` — a model-free unit test with the fingerprint string as
a constant. `corpus.json` = `{schema_version: 1, corpus_identity, snapshot, exclusions,
chunker, embedder_fingerprint, partial?, counts}`; `ATTRIBUTION.txt` = the four lines of
`attribution()` (`record.rs:237-245`) with `recorded_at` RFC 3339 UTC seconds.

## D10 — Snapshot verification and build staging

**Decision**: before reading a line, `simple.jsonl`'s byte count and sha256 are checked
against `reference/datasets/wiki-manifest.json` (`jsonl.bytes`, `jsonl.sha256`); a
mismatch names the file, both hashes and `scripts/fetch-wiki.sh`. The build writes to
`<out>.partial/`, removes a stale one at start, refuses an existing `<out>`, and renames
into place only after the sidecars are written (`build.rs:126-140, 285-317`). Layout as
008's: `<out>/index/` (with `corpus.json` inside), `<out>/ATTRIBUTION.txt`,
`<out>/wiki-build.json`. `search`/`about`/`measure` take `--artefact <out>` (default
`target/xt-wiki`) and read `index/`, `index/corpus.json`, `ATTRIBUTION.txt` from it.

Cost of the hash: ~1 s for 300 MB on this laptop; the Rust build pays it too.

## D11 — Inputs and their producers

| Input | Default | Override | Missing → the message names |
|---|---|---|---|
| the package | `import xtriever` | — | `cd python && .venv/bin/maturin build --release` + `uv pip install target/wheels/xtriever-*.whl` |
| the embedder | `reference/models/all-MiniLM-L6-v2` | `XTRIEVER_MODEL_DIR`, `--embedder` | `scripts/fetch-model.sh` |
| the re-ranker | `reference/models/ms-marco-MiniLM-L-6-v2` | `XTRIEVER_RERANK_MODEL_DIR`, `--reranker` | `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json` |
| the artefact | `target/xt-wiki` | `XTRIEVER_WIKI_ARTEFACT`, `--artefact` | `cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki` or `wikidemo build --limit N --out …` |
| the snapshot | `reference/datasets/wiki/simple.jsonl` | `--snapshot` | `scripts/fetch-wiki.sh` |
| the manifest | `reference/datasets/wiki-manifest.json` | `--manifest` | (in the tree) |
| the host goldens | `<artefact>/expected.json` | `--expected` | `cargo run --release -p xtriever-cli -- wiki expected …` (017 quickstart) |
| the queries | `reference/fixtures/008/queries.json` | `--queries` | (in the tree) |

The environment-variable names for the models are the ones `python/tests/conftest.py`
already uses. Paths are resolved relative to the repository root, found by walking up
from the demo's own file to the directory holding `Cargo.toml` and `.specify/` — the demo
is run from a checkout (spec assumption); an absolute path overrides.

## D12 — Footprint: `ru_maxrss`

**Decision**: `resource.getrusage(resource.RUSAGE_SELF).ru_maxrss` — the process's peak
resident size, bytes on macOS and KiB on Linux (normalised by `sys.platform`); printed
after every search and recorded as `peakBytes` with `peakMethod: "ru_maxrss"`.

**Rationale**: standard library, no `psutil`; a peak is what the phone records too
(`ledgerPeakBytes`). The 600 MB ceiling is recorded for comparison, not asserted (a
laptop is not the reference device).

## D13 — `measure`: the device comparison, on the host

**Decision**: 20 queries × depths `[0, 5, 10, 20]` × `k=10`, `explain=True`, in the goldens'
order, one warm-up search first (labelled, not counted); per (query, depth) the wall time.
Comparison as `DeviceMeasurementTests` (`swift/Xtriever/Tests/XtrieverTests/
DeviceMeasurementTests.swift:194-250`): every golden query and depth must have a response;
the goldens' depth set must equal the measured set; hit counts equal; every hit present on
the host side by id; `bm25` bits identical (lexical); fused order identical at depth 0;
dense and re-rank scores within 1e-3 per document matched by id; a score present on one
side only is a mismatch. The record also reports how many hits matched on *every* bit
(`score_bits`, `rerank_score_bits`, `rerank_combined_bits`) — on the machine that minted
the goldens this should be all of them, and the number is on record either way.

The per-depth medians and maxima, `medianFusedMs` (depth 0), `medianRerankedMs` (depth
10 — the demo default) and `medianRerankedAtEngineDefaultMs` (depth 20) go into the
record beside the 009/018 keys so the tables line up.

## D14 — Slice parity: the Rust build of the same N

**Decision**: `cargo run --release -p xtriever-cli -- wiki build --limit N --out
target/xt-wiki-slice-rs` and `wikidemo build --limit N --out target/xt-wiki-slice-py`; then
`wikidemo measure --artefact target/xt-wiki-slice-py --against target/xt-wiki-slice-rs`:
with `--against`, `measure` opens the second artefact instead of reading the host goldens
and compares live responses by the same rule as D13, plus the identity and the counts from
the two `corpus.json`s, writing a slice-parity record. (A separate subcommand for a
one-time check would be clutter; the comparison code is the same.)
N = 2,000 (≈ 3–6 k passages; 3–9 min at 93 ms/passage; the Rust side is seconds when the
cache hits, minutes otherwise).

**Rationale**: SC-005 and FR-014 — the recipe is demonstrably the shipped one at any N
without an 11-hour build. `--against` reuses the comparison code with a second live index
as the "goldens".

## D15 — Environment

**Decision**: `apps/python-wiki-demo/.venv` (gitignored), Python 3.12 via `uv venv
--python 3.12`; `uv pip install ../../target/wheels/xtriever-*.whl -e ".[test]"`;
`pyproject.toml` with `dependencies = ["xtriever>=0.1.0", "tokenizers==0.23.2"]` and
`[project.optional-dependencies] test = ["pytest>=8"]`; `[project.scripts] wikidemo =
"wikidemo.cli:main"`; build backend `setuptools` (already what `uv`/`pip` resolve for a
plain package — no new tool). Tests: `pytest` with the `models` marker and the
skip-with-reason collection hook copied from `python/tests/conftest.py`.

**Not** `python/.venv`: it has no `tokenizers`, and the package's test environment should
stay the package's. **Not** `reference/.venv-008`: the reference venvs are the fixture
generators'.

## D16 — Output layout

```
opened target/xt-wiki (427,947 passages, format 2) · embedder 169 ms · re-ranker 174 ms · open 340 ms · mmap
warm-up: the first search of a process pages the vectors in

fused (lexical + dense), 10 hits, 251 ms
 1. Sky blue                              834076#0  passage 1
    https://simple.wikipedia.org/wiki/Sky%20blue
    Sky blue is a shade of cyan. …
 2. Sky                                   2004#0    passage 1
    …

re-ranked (interpolate α 0.5, depth 10), 10 hits, 822 ms
 1. ↑1  Sky                               2004#0    passage 1
 2. ↓1  Sky blue                          834076#0  passage 1
 3. =   Sky                               2004#1    passage 2
 …
dropped from the head: 5163#3 (was 9)

stages: lexical 100 · dense 100 · re-rank 10 candidates, 10 scored · time limit ignored: no · engine 822 ms
wall: fused 251 ms · re-ranked 822 ms · total 1,073 ms · peak resident <n> MB
```

(Illustrative — the numbers are today's measurements from the pre-spec check; the layout is
the contract, `contracts/cli.md`.)

`--explain` adds under each hit the eight features in the engine's names
(`bm25.score`, `bm25.rank`, `dense.score`, `dense.rank`, `fused.score`, `rerank.score`,
`rerank.rank`, `rerank.combined` — the Swift `features()` list) with "not seen by this
stage" for `None`; `--snippet N` truncates passages to N characters with "…" and says so
in the header. Degradation prints as `degraded: <stage> (<reason>)`; a skipped re-rank as
`re-rank skipped (<reason>)`.

## D17 — Threads

**Decision**: recorded, not controlled: `RAYON_NUM_THREADS` when set, else `os.cpu_count()`
(candle's default) — the same `threadSource` wording as the 009 record. A `--threads` flag
would have to set the variable before the shared library loads; the README says to export
it.

## D18 — What is deliberately not built

- No persisted settings, no REPL, no `--json` output, no server.
- No Wikipedia-specific helpers added to the `xtriever` package (FR-019); the demo carries
  them, tested against the same cases.
- No use of the Rust embedding cache; no fetching of anything.
- No CI job: the demo's model-free tests could run in CI, but the standing rule is that no
  job needs the models, and a job that runs only the chunker replay would need the wheel
  built — a CI cost for no coverage the local gate does not give. The tests run locally
  under Rule 5 and their counts go into the PR.
