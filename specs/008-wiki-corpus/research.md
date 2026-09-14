# Research: The Wikipedia Corpus and Shipped Index

**Feature**: `008-wiki-corpus` | **Date**: 2026-09-13 | **Spec**: [spec.md](./spec.md)

Every decision below was checked against the pinned versions in `Cargo.lock` (tantivy 0.26.2,
tokenizers 0.23.2, candle 0.9.2) or measured on the actual snapshot (Rule 1, Principle IV).
Measurements were taken on 2026-09-13 with the scratch script recorded under D4.

## D1. The snapshot: the Hugging Face `wikimedia/wikipedia` parquet, `20231101.simple`

**Decision**: pin `https://huggingface.co/datasets/wikimedia/wikipedia/resolve/main/20231101.simple/train-00000-of-00001.parquet`
— 156,885,218 bytes, SHA-256 `31bded16768a47c286becd292079122f5d7d4397a17b87d4250a00ccd581e6f0`
(the server's own linked etag agrees), 241,787 rows, columns `id`, `url`, `title`, `text`
(plain text, markup already stripped by the dataset's build). Fetched by `scripts/fetch-wiki.sh`
into `reference/datasets/wiki/` (gitignored), verified against `reference/datasets/wiki-manifest.json`.

**Rationale**: The Wikimedia XML dumps carry wikitext — templates, tables, references — and
turning that into text is a parser project with no reference implementation to pin to. The
parquet is the same content already reduced to `id/url/title/text`, one file, 157 MB, hashed.
Its `id` is the MediaWiki page id (numeric, unique — verified), the stable identifier FR-004 asks
for; `url` is the canonical article URL (see D6).

**Alternatives**: `simplewiki-latest-pages-articles.xml.bz2` (wikitext, moving target, no
pinned hash); a curated English subset (needs an article list nobody has written).

## D2. Parquet is read by Python in `reference/`, not by Rust

**Decision**: `reference/wiki_to_jsonl.py` (pyarrow, pinned in `reference/requirements-008.txt`,
venv `.venv-008`) converts the parquet to `simple.jsonl` — one object per line,
`{"id","url","title","text"}`, keys in that order, `ensure_ascii=False`, rows in parquet order.
The manifest pins both the parquet and the derived JSONL (bytes, SHA-256, line count); the
fetch script verifies both; the Rust build reads only the JSONL.

**Rationale**: the `parquet`/`arrow` crates are ~40 dependencies of compile time for one read;
the repository already treats `reference/` Python as the pinned data tooling (BEIR, models,
fixtures). A deterministic JSONL with a pinned hash is as verifiable as the parquet.

**Alternatives**: `parquet` crate in `xtriever-cli` (rejected: dependency weight for one file).

## D3. Exclusions: disambiguation pages only, by two stated rules; no redirects or empties exist

**Decision**: exclude an article if its title ends with ` (disambiguation)` **or** its first
300 characters contain `may refer to` or `may mean`. Everything else is in. The build record
counts each rule's hits separately.

**Measured**: 0 empty bodies, 0 `#REDIRECT` bodies (the dataset's build drops redirects),
2,350 disambiguation-like articles by the rules above. The rules are heuristics and are stated
as such; a disambiguation page that escapes them is indexed as an ordinary (short) article.

## D4. Corpus measurements that set the rest of the plan

Scratch script (pyarrow + tokenizers 0.23.2 with the pinned embedder `tokenizer.json`,
truncation **disabled** — the shipped `tokenizer.json` truncates at 128 by default, which
silently capped a first pass at 128 pieces; the Rust dense crate sets its own 256):

| Quantity | Value |
|---|---|
| Articles | 241,787 |
| Body text | 269 MB UTF-8; mean 1,113 B |
| Words per article p50 / p90 / p99 / max | 81 / 334 / 1,546 / 32,481 |
| Articles ≤ 50 words | 31.6 % |
| Passages by a **word** bound of 120 / 150 / 180 | 563,080 / 477,741 / 423,916 |
| Word-pieces per passage at 150 words, `title\n\nbody`, p50 / p90 / p99 / max | 144 / 327 / 387 / 1,589 |
| Passages over 256 pieces at 120 / 150 / 200 words | 11.8 % / 15.0 % / 21.9 % |

**Consequence**: a bound in words cannot meet FR-006 (100 % of passages inside the window);
Simple English's names, dates, numbers and table residue tokenise at 1.5–3 pieces per word with
a long tail. The bound has to be in word-pieces — D5.

## D5. The chunker: a pure algorithm over a caller-supplied cost function

**Decision**: `xtriever_analysis::chunk::chunk(text, budget, cost) -> Vec<Passage>` where
`cost: &dyn Fn(&str) -> usize` prices a unit of text and `budget` is the maximum cost of one
passage. The algorithm is fixed and byte-exactly specified (contracts/chunker.md):
paragraphs (split on blank lines) are packed greedily into a passage while the summed cost
fits; a paragraph that does not fit alone is split into sentences (after `.`, `!`, `?` followed
by whitespace) and packed the same way; a sentence that does not fit alone is split into
whitespace-delimited words; a single word over the budget is split at UTF-8 character
boundaries by halving until each part fits. Each passage records the byte range of its first
and last unit in the original body; passages tile the body in order (whitespace between units
is not inside any range — the ranges are of the units' text, and the passage's text is the
units joined by one space, paragraphs by a newline).

The **build** prices with the embedder: `cost(s) = MiniLmEmbedder::token_count(s)` (D7), budget
`254 − token_count(title)` (256 minus `[CLS]`/`[SEP]`, minus the title line — D6). The **golden
fixtures** price two ways: `cost = word count` with small budgets (exercises every branch,
independent of any model) and the real tokenizer cost through the Python reference (the same
`tokenizers` 0.23.2, the pinned `tokenizer.json`), so the fixtures pin both the algorithm and
its behaviour under the production cost.

WordPiece cost is additive over whitespace-separated units — BERT's basic tokenizer splits on
whitespace and punctuation before WordPiece runs per word (`tokenizers` `BertPreTokenizer` +
`WordPiece`), so `cost(a + " " + b) == cost(a) + cost(b)`. The build does not rely on that
silently: FR-006's verification tokenises every finished passage and fails the build on any
passage over 256.

**Rationale**: keeps the chunker in a pure crate (FR-012, Principle III — no `tokenizers`
there) while letting the build bound in the units that matter. One algorithm, one code path,
reproduced independently in `reference/gen_008_fixtures.py`.

**Alternatives**: `text-splitter` crate (has a tokenizers-aware splitter) — rejected: its
segmentation rules are the crate's, not ours; the Python reference would have to copy them to
produce goldens, which is not an independent oracle; and it would not be a pure-crate
dependency. A words bound (D4: fails FR-006). A characters bound (would need ~250 characters
to be safe for non-Latin runs — passages of a sentence each).

## D6. Passage text is `Title\n\nbody`; title is also its own boosted field; the URL is derived

**Decision**: the indexed document has two lexical fields — `title` (`Text`, `standard_en`,
boost 2.0) and `text` (`Text`, `standard_en`, boost 1.0) — and `dense_fields = ["text"]`, where
`text` is the article title, a blank line, and the passage body. The pipeline's passage store
therefore holds exactly `text`, so `Hit.text` begins with the title line and the app splits at
the first `\n\n`. `chunk = ChunkInfo { parent: <page id>, ordinal, byte_range: (start, end) }`
where the range is the body passage's range in the article's `text` column. The article URL is
`https://simple.wikipedia.org/wiki/` + the title percent-encoded (UTF-8; unreserved
`A–Z a–z 0–9 - _ . ~` and `/` kept, everything else `%XX`) — **verified exact for 241,787 /
241,787 articles** against the snapshot's `url` column; the build re-verifies and fails on a
mismatch.

**Rationale**: the pipeline returns passage text and chunk provenance but no stored fields
(no field retrieval exists in `xtriever-lexical`, and adding one is a stage + pipeline + FFI
change this feature does not need). Putting the title on the first line gives the hit its
title with no wire change and gives both retrieval stages the title's terms; deriving the URL
removes a 25 MB sidecar the app would have to load. The double presence of title terms in
BM25 (own field + inside `text`) is a known, small effect; this corpus has no quality oracle
(Q2) and the BEIR baselines are unaffected (SciFact keeps its own configuration).

**Alternatives**: a `articles.jsonl` sidecar keyed by parent id (rejected: 400k-entry lookup
in the app); a stored `url` `Keyword` field (rejected: 25 MB nothing can read); adding stored
field retrieval to the pipeline (rejected: out of scope, three crates).

## D7. `MiniLmEmbedder::token_count` — a second, unbounded tokenizer in the dense crate

**Decision**: `xtriever-dense` gains `pub fn token_count(&self, text: &str) -> Result<usize>`,
served by a second `Tokenizer` built at load from the same pinned `tokenizer.json` with
`with_truncation(None)` and `with_padding(None)` (`tokenizers::Tokenizer::with_truncation`,
`with_padding` — the builder-style methods at `tokenizer/mod.rs:426,433` of 0.23.2) and
`add_special_tokens = true`, so the count includes `[CLS]` and `[SEP]` and is exactly the
sequence length the embedder would need to see the text whole.

**Rationale**: the embedder's own tokenizer pads and truncates to 256 (`embed_one` asserts
exactly 256 positions), so it cannot say "this text is longer than my window". The question
belongs to the embedder, not to the build tool; `xtriever-dense` is a leaf crate and may
depend on `tokenizers` already. Additive change, no trait touched.

**Alternatives**: the build loading `tokenizer.json` with `tokenizers` itself (rejected:
duplicates the pinned-file verification and the crate's tokenizer configuration).

## D8. The build tool is `xtriever-cli`'s first command; clap for arguments

**Decision**: `crates/xtriever-cli` (today an empty placeholder) becomes the `xtriever` binary
with a `wiki` subcommand family: `wiki build`, `wiki verify`, `wiki expected`. Arguments via
`clap` (`cargo add clap --features derive`; derive `Parser`/`Subcommand`/`Args`). Errors via
`anyhow` (a binary — Principle VII). Dependencies: `xtriever-core`, `-analysis`, `-pipeline`
(`mmap`), `-dense` (`mmap`), `serde`, `serde_json`, `sha2`, `clap`, `anyhow`. Dependency
direction `core` ← stages ← `pipeline` ← `cli` holds.

**Rationale**: the spec allows the CLI crate or the eval harness; the eval harness is a
BEIR-shaped example with a hand-rolled argument map, and a corpus builder is a real command a
person runs, not an evaluation. `clap` is the boring choice for a real CLI (Rule 7).

## D9. Streaming build with a sharded, resumable embedding cache

**Decision**: the build streams: read JSONL → exclude → chunk (D5) → group passages into
shards of 4,096 in corpus order → for each shard, look up
`<cache>/<embedder-fingerprint-sha256[..16]>/shard-<NNNNN>.f32` whose sidecar
`shard-<NNNNN>.json` records `{ "key": sha256(length-prefixed passage texts), "count" }`; on a
key match the vectors (`count × dim` little-endian `f32`) are read, else the shard is embedded
one passage at a time (`Embedder::embed(&[text], TextKind::Passage)`, the determinism path)
and written (`.tmp` + rename) → `HybridIndex::add_embedded(&batch)` → next shard. After the
last shard: `commit()`, then `merge()` (D10), then the identity file and build record. The
output directory is written under `<out>.partial/` and renamed to `<out>` only after the
record is written, so an interrupted build leaves nothing openable (FR-009).

Cache key excludes the corpus identity on purpose: a chunker change that leaves most passages
byte-identical still re-uses every unchanged shard whose texts hash the same; a changed
passage changes its shard's key and only that shard is re-embedded.

**Rationale**: 480k passages at the dense stage's measured ~94 ms each (004: FiQA 57,638 in
~90 min; the embedder always runs 256 positions, so cost is per passage, not per token) is
**~12.5 h** single-threaded; a build that cannot resume is not a build anyone finishes.
Holding all vectors in memory (740 MB) plus all texts is avoidable by streaming.

**Alternatives**: reuse the eval harness's `EmbeddingCacheKey`/`FlatIndex` cache (all-or-nothing:
one `cache.json` written at the end — a crash at hour 11 loses hour 0–11); embedding in
batches (rejected: the dense stage is one-at-a-time by design for determinism, 004).

## D10. The build ends with a merge to one segment — a new `HybridIndex::merge`

**Decision**: `xtriever-pipeline` gains `pub fn merge(&mut self) -> Result<()>` delegating to
`TantivyIndex::merge` (exists: `xtriever-lexical/src/index.rs:128`, "merge all segments into
one", used by the 002 determinism tests). The build calls it after `commit()`.

**Rationale**: with the lexical writer's 15 MB budget, 480k documents produce dozens of
segments and background merges that may still be running when the process exits; a shipped
artefact should be one segment (search cost on the phone, deterministic `DocAddress` order
equal to `DocId` order, no leftover merge files). BM25 scores do not depend on segment layout
(tantivy computes IDF and average field length from searcher-wide totals) — and the SciFact
baseline is re-run to show it (quickstart Step 6).

## D11. Read-only open: a tantivy `Directory` wrapper that never takes the lock

**Decision**: `xtriever-lexical` gains `ReadOnlyDirectory` (private) wrapping
`tantivy::directory::MmapDirectory` and implementing `tantivy::Directory` by delegation for
`get_file_handle`, `exists`, `atomic_read`, `watch`, `sync_directory`, with `open_write`,
`delete`, `atomic_write` returning an I/O error ("read-only index") and **`acquire_lock`
returning `Ok(DirectoryLock::from(Box::new(())))`** (the public
`impl<T: Send + Sync + 'static> From<Box<T>> for DirectoryLock`, `directory/directory.rs:49`).
`TantivyIndex::open_read_only(dir)` builds it and opens with `Index::open(directory)`
(`index/index.rs:510`, `T: Into<Box<dyn Directory>>`); mutations on such an index fail at the
writer lock with the same "read-only index" error. `xtriever-pipeline` gains
`HybridIndex::open_with(dir, embedder, OpenOptions { mapped: bool, read_only: bool })`; the
existing `open`/`open_mapped` delegate to it. `xtriever-ffi`'s `IndexHandle::open` opens with the lock when the directory permits and falls
back to `read_only: true` only on a `PermissionDenied` lock file — review round 1 pointed out
that a non-mutating handle does not make a writable directory immutable, and the lock is what
protects a reader from another writer's garbage collection; a directory this process cannot
write is the one case where no writer of its rights can exist. `xtriever-lexical` maps that
lock failure to `Error::Io` so the fallback matches on a fact, not a message.

**Where the lock is taken** (tantivy 0.26.2, read from source): `IndexReader` creation →
`open_segment_readers` → `directory.acquire_lock(&META_LOCK)` (`reader/mod.rs:194`), which
`Directory::acquire_lock`'s default implementation serves by `open_write`-ing
`.tantivy-meta.lock` (`directory/directory.rs:74–87`) and deleting it on drop. `MmapDirectory::open`
and `ManagedDirectory::wrap` only read (`mmap_directory/mod.rs:236–260`,
`managed_directory.rs:65–90`). The meta lock exists to keep a concurrent writer's garbage
collection from deleting segment files while a reader opens them; an index nobody can write
has no such writer, so the lock guards nothing.

**Consequences**: 007's F-001 is resolved — the app opens the index inside its bundle;
`XtrieverIndex.writableCopy` and its tests are removed from the Swift package; the on-device
footprint of this corpus is one copy, not two; the device record gains "opened in place" and
the bundle's index size instead of copy time. A Rust test opens a `chmod 0o555` index
directory read-only and searches it; the 007 `readonly.rs` snapshot test keeps proving nothing
is written. Not an on-disk format change and not a core trait change; the error semantics of
`open` gain a success case that was a defect (Principle V row).

**Alternatives**: copy only `lexical/` and symlink the rest from Application Support into the
bundle (rejected: symlink targets change with every app update; still a 250 MB copy; Swift-only
fix for a Rust-side defect); keep the whole-directory copy (rejected: 1.4 GB duplicated on the
phone for a zero-byte lock file).

## D12. Corpus identity and build record are sidecars the pipeline ignores

**Decision**: the index directory gains `corpus.json`: `{ schema_version: 1, corpus_identity,
snapshot: { edition, date, parquet_sha256, jsonl_sha256 }, exclusion_rules, chunker: { version,
budget_rule }, embedder_fingerprint, counts }`, where `corpus_identity = sha256` over the
canonical JSON of everything but itself and `counts`. `HybridIndex::open` does not read it
(a test asserts an index with the sidecar opens and one without it too). The build record
`wiki-build.json` (identity, counts per exclusion rule, passages, per-phase wall time, thread
count, host, sizes per file, cache hits/misses) and `ATTRIBUTION.txt` are written **beside**
the index directory, and the record is committed to `specs/008-wiki-corpus/build-record.json`.

**Rationale**: the descriptor `xtriever-pipeline.json` is format v2 (ADR-0008); adding fields
would be a format change needing an ADR for what is metadata about the corpus, not the index.

## D13. Resources, harness and measurement queries

**Decision**: `scripts/build-ios-package.sh --with-wiki` stages `XtrieverData/wikipedia/{index/,
wiki-build.json, ATTRIBUTION.txt, queries.json, expected.json}` from `target/xt-wiki/` and fails
if the staged resources (models + wikipedia) exceed the **2.0 GB** bundle budget (an estimate
from D4/D9: ~740 MB vectors + ~300 MB passages + ~250 MB lexical + 174 MB models ≈ 1.5 GB;
revisited against the measured artefact in the report). `expected.json` is produced by
`xtriever wiki expected --index --queries --out`, the same shape `fixture_index --scifact`
writes (FFI-side goldens for parity: hits with score bits per depth 0/5/20).
`DeviceMeasurementTests` selects the corpus from `TEST_RUNNER_XTRIEVER_CORPUS=scifact|wikipedia`
(default `scifact`, so 007's runs stay reproducible) and reads `XtrieverData/<corpus>/…`; the
record gains `corpus`, `indexBytes`, `openedInPlace`. Twenty measurement queries live in
`reference/fixtures/008/queries.json` — plain questions, no answers (FR-013).

## D14. What is deliberately not done

- No quality oracle (owner decision Q2) — the report states it.
- No field retrieval, no wire change in the FFI, no format change, no core trait change.
- No batching in the embedder; no threads in the build beyond candle's own — the ~12 h is
  paid once and cached.
- No download-on-first-launch: the index ships in the bundle (007's "shipped index" decision).
- CI runs the chunker fixtures and the manifest's schema checks only; never the fetch or the
  build (standing rule).
