# Tasks: The Wikipedia Corpus and Shipped Index

**Input**: Design documents from `/specs/008-wiki-corpus/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/chunker.md](./contracts/chunker.md),
[contracts/cli.md](./contracts/cli.md), [contracts/artefact.md](./contracts/artefact.md),
[quickstart.md](./quickstart.md), [ADR-0010](../../docs/adr/0010-device-rss-ceiling-600mb.md)

**Tests**: **Mandatory** (Principle II, spec FR-018). Phase 2 lands every fixture and every
test red: the chunker goldens against a module that does not exist, the read-only / `open_with`
/ `merge` / `token_count` tests against APIs that do not exist, the CLI unit tests against a
binary with no `wiki` command. Story phases contain implementation only and end with the task
that turns their tests green.

**Organization**: Setup (manifest, snapshot tooling, crate scaffolds) → Red suite → US2
(passages: the chunker and the embedder's window) → US1 (the build: pipeline additions, the
CLI, the development build, then the full ~12 h build) → US3 (shipping: read-only open, Swift,
staging) → US4 (device) → Polish. US2 precedes US1 because the build is the chunker's first
caller. The five-PR split from plan.md: **PR 1** = Phases 1–2, **PR 2** = Phase 3 + T024–T025,
**PR 3** = T026–T034, **PR 4** = Phase 5, **PR 5** = Phases 6–7 (the full build runs between PR 3
and PR 5 and its record lands in PR 5).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model sections
  and the three contracts — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

Crates `crates/xtriever-{analysis,lexical,pipeline,dense,ffi,cli}/`; Python reference
`reference/` (`wiki_to_jsonl.py`, `gen_008_fixtures.py`, `requirements-008.{in,txt}`,
`.venv-008/`); snapshot cache `reference/datasets/wiki/` (gitignored); manifest
`reference/datasets/wiki-manifest.json`; fixtures `reference/fixtures/008/`; scripts
`scripts/{fetch-wiki.sh,build-ios-package.sh}`; Swift package `swift/Xtriever/`; artefact
`target/xt-wiki/`, cache `target/xt-wiki-cache/`; run records `specs/008-wiki-corpus/runs/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pin the snapshot, make it fetchable and convertible, and scaffold the crates the
stories fill — nothing here is behaviour.

- [X] T001 Write `reference/datasets/wiki-manifest.json` exactly per data-model "Snapshot manifest": edition `simple`, `snapshot_date` `2023-11-01`, licence CC BY-SA 4.0 with URL, `parquet` `{url, bytes: 156885218, sha256: "31bded16768a47c286becd292079122f5d7d4397a17b87d4250a00ccd581e6f0", rows: 241787}`, `jsonl` `{file: "simple.jsonl", bytes: 0, sha256: "", lines: 241787, converter: "reference/wiki_to_jsonl.py", converter_sha256: ""}` (filled by T005), the three exclusion rules verbatim (`title_suffix " (disambiguation)"`, `lead_contains "may refer to" within_chars 300`, `lead_contains "may mean" within_chars 300`); add `reference/datasets/wiki/` and `target/xt-wiki*` and `swift/Xtriever/Sources/Xtriever/XtrieverData/wikipedia/` (already covered by the `XtrieverData/` rule — note it) to `.gitignore` with a comment mirroring the BEIR one
- [X] T002 [P] Write `reference/requirements-008.in` (`pyarrow`, `tokenizers==0.23.2` — the Rust crate's pin, stated in a comment as in `requirements-004.in`) and generate `reference/requirements-008.txt` with hashes via the same tool `setup-reference-venv.sh` expects (`uv pip compile --generate-hashes`); run `scripts/setup-reference-venv.sh 008` and record the interpreter version it printed
- [X] T003 [P] Write `reference/wiki_to_jsonl.py`: reads the parquet with `pyarrow.parquet.read_table`, writes `simple.jsonl` with one `json.dumps({"id","url","title","text"}, ensure_ascii=False)` object per line in parquet row order, keys in that order, `\n` line ends; refuses to overwrite a differing existing file; prints line count, bytes and SHA-256 (research D2)
- [X] T004 Write `scripts/fetch-wiki.sh` mirroring `scripts/fetch-beir.sh`: `curl -L` the parquet into `reference/datasets/wiki/` unless present, `verify` bytes + sha256 against the manifest (same function shape, exit 1 with both hashes), run `reference/.venv-008/bin/python reference/wiki_to_jsonl.py` unless `simple.jsonl` exists, verify the JSONL against the manifest's `jsonl` entry **unless its sha256 is empty** (first run), in which case print the measured bytes/sha256/lines and the converter's sha256 and exit 1 with "pin these in the manifest" — the pin is a human action, never automatic
- [X] T005 Run `scripts/fetch-wiki.sh` once, pin `jsonl.bytes`, `jsonl.sha256`, `converter_sha256` in `reference/datasets/wiki-manifest.json` from its output, run it again and confirm both verifications pass and nothing is re-downloaded or re-converted; record the JSONL size in a comment of the manifest's `note`
- [X] T006 [P] Scaffold `crates/xtriever-cli` **with cargo**: `cargo add -p xtriever-cli clap --features derive`, `cargo add -p xtriever-cli anyhow serde --features serde/derive`, `cargo add -p xtriever-cli serde_json sha2`, `cargo add -p xtriever-cli --path crates/xtriever-analysis`, `--path crates/xtriever-pipeline --features mmap`, `--path crates/xtriever-dense --features mmap`; `cargo add -p xtriever-cli --dev tempfile`; set `[[bin]] name = "xtriever" path = "src/main.rs"`, `description = "Xtriever: command-line tools (corpus builds)"`; `src/main.rs` with `clap` `Parser` → `Commands::Wiki(WikiArgs)` → `WikiCommand::{Build, Verify, Expected}` whose handlers return `anyhow::bail!("not implemented")`; module files `src/wiki/{mod,manifest,rules,chunking,cache,build,verify,expected,record}.rs` empty except `//!` docs (research D8)
- [X] T007 [P] Add `proptest` as a dev-dependency of `crates/xtriever-analysis` **with cargo** (`cargo add -p xtriever-analysis --dev proptest`); confirm `cargo tree -p xtriever-analysis -e normal` still shows only `xtriever-core` (the pure-crate rule is about normal deps); update the crate `description` to "Xtriever: text analysis — chunking (pure Rust, std-only)"

---

## Phase 2: Red Suite (Rule 4 — committed failing)

**Purpose**: Every oracle exists before any implementation. **Checkpoint: PR 1.**

- [X] T008 Write `reference/gen_008_fixtures.py`: an independent implementation of contracts/chunker.md steps 1–8 in Python (regex-free where the contract is character-level; the sentence split is `re.split(r"(?<=[.!?])\s+")`-equivalent, stated); **set A** — 12 hand-written bodies covering: single paragraph under budget; two paragraphs packed; paragraph over budget split to sentences; sentence over budget split to words; a single word over budget split to fragments (a 40-character token at budget 3 words → by character halving, cost = words = 1 each… use a body where cost is *characters* for this case? No — set A's cost is words, so an over-budget single word is impossible; cover fragments in set B only and say so); `Mr. Smith` two-sentence rule; blank lines with `\t`/`\r` runs; leading/trailing whitespace; internal whitespace runs collapsed; empty body; budget 0; CJK and combining characters (ranges in bytes); a body ending without a terminator — with costs = words and budgets 1, 3, 8, 40 → `reference/fixtures/008/chunk_a.json` `[{name, body, budget, passages: [{text, byte_range, cost}]}]`; **set B** — 8 real articles from `simple.jsonl` chosen by id (state them) including one over 5,000 words and one with a 900-piece token, cost = `len(tokenizer.encode(unit, add_special_tokens=True).ids) − 2` (content pieces) with the pinned `tokenizer.json`, truncation disabled, budget `256 − token_count(title)` → `chunk_b.json` with the same shape plus `title`; `manifest.json` with sha256 of both and of the generator; every passage text in B re-tokenised with the title line and asserted `≤ 256` by the generator itself (research D5)
- [X] T009 [P] Write `crates/xtriever-analysis/tests/chunk_golden.rs`: load both fixture files (serde_json dev-dep — `cargo add -p xtriever-analysis --dev serde_json serde --features serde/derive`), run `xtriever_analysis::chunk::chunk(body, budget, &cost)` with `cost = |s| s.split_whitespace().count()` for A and, for B, a cost table the fixture carries (`"unit_costs": {unit: cost}` — add to the generator so the Rust test needs no tokenizer; every unit the algorithm will price must be in the table, and a missing unit panics the test naming it), compare `text`, `byte_range`, `cost` byte for byte; verify the fixture manifest hashes first. Red: no `chunk` module
- [X] T010 [P] Write `crates/xtriever-analysis/tests/chunk_prop.rs` (proptest): for bodies from a generator mixing words, punctuation, `\n`, `\n\n`, tabs and CJK, and budgets 1..=64 with cost = words: tiling (ranges increasing, non-overlapping, every non-whitespace byte inside exactly one range), bound (`cost ≤ budget` or a single-fragment passage), determinism (two calls equal), text idempotence (re-chunking a passage's text yields one passage with the same text). Red
- [X] T011 [P] Write `crates/xtriever-dense/tests/token_count.rs` (`#[ignore = "needs the model"]`, release): `MiniLmEmbedder::load` both paths; `token_count("")` = 2; `token_count("hello world")` = 4; a 3,000-word text returns a count > 256 (not 256 — proves no truncation); `token_count(a) + token_count(b) − 2 == token_count(a + " " + b)` (content pieces are additive) for ten fixture pairs (WordPiece additivity, D5); equals `tokenizer.json` Python counts in `chunk_b.json`'s `unit_costs` for every unit there. Red: no such method
- [X] T012 [P] Write `crates/xtriever-lexical/tests/read_only.rs`: build a small index in a tempdir, `chmod` the directory tree `0o555` (`std::fs::set_permissions`; skip with a message on Windows), `TantivyIndex::open_read_only(dir)` succeeds, `search` returns the same hits as a normal open of a writable copy, `add`/`commit`/`merge` return `Error::Io` whose message contains `"read-only index"`, and a `snapshot(dir)` of `(path, len, mtime)` (the 007 `readonly.rs` helper, copied) is unchanged after open + search; a normal `open` of the same `0o555` directory still fails (documents the defect the wrapper fixes); restore permissions in a `Drop` guard so tempdir cleanup works. Red: no `open_read_only`
- [X] T013 [P] Write `crates/xtriever-pipeline/tests/open_with.rs`: `HybridIndex::open_with(dir, embedder, OpenOptions { mapped: false, read_only: true })` on a `0o555` fixture index opens and searches identically to `open`; `add`/`commit`/`merge` on it return `Error::Io` "read-only index"; `open_with(.., { mapped: true, read_only: false })` equals `open_mapped`; an index directory containing an extra `corpus.json` opens (sidecar tolerance, D12) and one without it too. And `tests/merge.rs`: after `add` in three commits (three segments — assert via a count the lexical crate exposes for tests, or via the segment file count under `lexical/`), `merge()` leaves one segment and every search result and score bit-identical to before the merge (D10). Red
- [X] T014 [P] Write `crates/xtriever-cli/tests/{manifest,rules,cache,url}.rs`: `manifest.rs` — parses `reference/datasets/wiki-manifest.json`, rejects a manifest with an unknown rule kind, `verify_file(path, bytes, sha256)` errors name the path and both hashes; `rules.rs` — the three rules against titles/leads: `"Flame (disambiguation)"` excluded by rule 0, a lead with `may refer to` at char 250 excluded by rule 1, at char 350 not excluded, `may mean` by rule 2, first matching rule wins and is the one counted; `cache.rs` — a shard written with `write_shard(dir, n, key, vectors)` is read back bit-identical, a sidecar with a different key / count / fingerprint or a `.f32` of the wrong length is a miss, a `.tmp` left behind is ignored; `url.rs` — `derive_url(title)` for `"April"`, `"Alan Turing"` → `%20`, `"Church (building)"` → `%28`/`%29`, `"Dutton's Speedwords"` → `%27`, `"AC/DC"` keeps `/`, `"Biel/Bienne"`, a title with `é` → `%C3%A9`, `"~"` kept, upper-case hex (D6). Red: no such modules
- [X] T015 [P] Write `reference/fixtures/008/queries.json`: 20 plain questions a person would type about Simple-English-Wikipedia-sized topics (geography, science, history, arts, sport, technology — e.g. "why is the sky blue", "who was the first person on the moon", "how do vaccines work"), ids `q01`–`q20`, **no expected answers**; a comment-free JSON array per data-model (FR-013)
- [X] T016 [P] Write `swift/Xtriever/Tests/XtrieverTests/WikipediaTests.swift`: skips unless `HarnessResources.wikipediaIsBundled`; opens `HarnessResources.wikipediaIndexDirectory` **directly** (no copy) with both models; searches `queries.json`'s first query with `k: 5`; asserts every hit's `text` contains `"\n\n"`, `Hit.titleAndPassage` splits it, `Hit.wikipediaURL` starts with `https://simple.wikipedia.org/wiki/` and equals the derivation of the title (a Swift port of D6's rule, tested against the same eight titles as T014), `chunk?.parent` is a decimal string and `externalId == "\(parent)#\(ordinal)"`; and a test that opening the bundled index leaves the bundle untouched (file list + sizes). Red: no `wikipediaIsBundled`, no `titleAndPassage`, no `wikipediaURL`, and `open` on the read-only bundle fails today
- [X] T017 Red checkpoint: `cargo nextest run -p xtriever-analysis -p xtriever-lexical -p xtriever-pipeline -p xtriever-cli` all red for the new tests and green for the old; `cargo nextest run -p xtriever-dense --release --run-ignored only -E 'test(token_count)'` red; the Swift test does not compile (recorded as its red state); `./scripts/check-no-stubs.sh` **FAILS** on the `bail!("not implemented")` stubs (expected here, PASSes after T034); `reference/fixtures/008/manifest.json` hashes verified. Commit as **PR 1**

---

## Phase 3: User Story 2 — Articles become passages a phone can show (Priority: P1) 🎯 MVP

**Goal**: The pure chunker and the embedder's window count — the two things that decide what
the index contains.

**Independent Test**: chunker goldens A and B byte-identical; properties hold; `token_count`
tests green in release.

- [X] T018 [US2] Implement `crates/xtriever-analysis/src/chunk.rs` per contracts/chunker.md: `pub struct Passage { pub text: String, pub byte_range: (u64, u64), pub cost: usize }`, `pub fn chunk(body: &str, budget: usize, cost: &dyn Fn(&str) -> usize) -> Vec<Passage>`; private unit types `Paragraph`/`Sentence`/`Word`/`Fragment` each carrying `(text, start, end)` in body bytes; packing per step 5 with the "finer level for that unit only" rule; text joining per step 6 (paragraph boundary inside a passage = `\n`); fragment splitting at the first `is_char_boundary` at or after `len / 2` (contract step 4); no `unwrap`/`expect`; `pub mod chunk` in `lib.rs` with a module doc stating the contract's eight steps in one paragraph each; T009 and T010 green
- [X] T019 [US2] Implement `MiniLmEmbedder::token_count` in `crates/xtriever-dense/src/embedder.rs`: at `load`, build a second `Tokenizer` from the same verified `tokenizer.json` with `with_truncation(None)?` and `with_padding(None)` (research D7, citations in plan Rule 1); `pub fn token_count(&self, text: &str) -> Result<usize>` = `encode(text, true).get_ids().len()`; doc comment states it includes `[CLS]`/`[SEP]` and never truncates; `MODEL_ID` unchanged (no embedding behaviour changes — say so in the doc); T011 green in release
- [X] T020 [US2] Run `cargo nextest run -p xtriever-analysis` and `cargo nextest run -p xtriever-dense --release --run-ignored only -E 'test(token_count)'`; `./scripts/check-containment.sh` still PASS (analysis has no clock, no unsafe, no tokenizer); `cargo tree -p xtriever-analysis -e normal --prefix none` = `xtriever-core` only. ⛔ Any golden mismatch is a report (which side of the contract is ambiguous), never an edited fixture

---

## Phase 4: User Story 1 — One command builds a pinned, reproducible Wikipedia index (Priority: P1)

**Goal**: The pipeline additions the build needs, the CLI, a development build proving
resumability and reproducibility, then the full build.

**Independent Test**: two `--limit 2000` builds give identical `corpus.json` and identical
`expected.json`; an interrupted build leaves no `<out>` and the re-run hits every finished shard.

- [X] T021 [US1] Implement `HybridIndex::open_with` and `OpenOptions { pub mapped: bool, pub read_only: bool }` in `crates/xtriever-pipeline/src/index.rs` (D11's pipeline half — the lexical half lands in T035; until then `read_only: true` on a writable directory must still work through a `TantivyIndex::open_read_only` that T035 provides — so **T021 depends on T035's lexical API**: implement T035 first if the PR order allows, else stub `open_read_only` as `open` and note it); `open`/`open_mapped` delegate; `pub fn merge(&mut self) -> Result<()>` delegating to the lexical `merge` (D10) and refusing on a read-only index; `#[derive(Clone, Copy, Debug, Default)]` on `OpenOptions`; docs; the sidecar-tolerance behaviour needs no code (confirm `open` reads only named files) — T013 green
- [X] T022 [US1] Write the `merge` determinism check into `crates/xtriever-pipeline/tests/merge.rs` (already red from T013) and make it green; then re-run the SciFact hybrid-rerank baseline after a merged index vs the committed baseline (`beir run … --index-dir /tmp/scifact-merged` after calling merge in a one-off — or simply assert T013's bit-identical scores across a merge on the 40-document fixture; record which was done)
- [X] T023 [US1] Implement `crates/xtriever-cli/src/wiki/manifest.rs` (serde structs per data-model "Snapshot manifest", `load(path)`, `verify_file(path, bytes, sha256) -> anyhow::Result<()>` with both hashes in the error, `sha256_file` streaming), `rules.rs` (`enum Rule { TitleSuffix(String), LeadContains { value, within_chars } }`, `fn excluded_by(&[Rule], title, text) -> Option<usize>` first match, `within_chars` counted in `chars`, not bytes), `url.rs` (`derive_url(title) -> String` per D6: percent-encode every UTF-8 byte outside `A-Za-z0-9-_.~/` as `%XX` upper-case); T014's `manifest`, `rules`, `url` tests green
- [X] T024 [US1] Implement `crates/xtriever-cli/src/wiki/cache.rs` per data-model "Embedding cache": `cache_dir_for(root, embedder_fingerprint)` = `root/<first 16 hex of sha256(fingerprint)>`, `shard_key(texts: &[&str])` = sha256 of texts joined by `\0`, `read_shard(dir, n, key, count, dim, fingerprint) -> Option<Vec<f32>>` (sidecar match + exact `.f32` length, else `None`), `write_shard(...)` via `.tmp` + rename for both files (sidecar last); little-endian `f32` via `to_le_bytes`/`from_le_bytes`; T014's `cache` test green
- [X] T025 [US1] Implement `crates/xtriever-cli/src/wiki/chunking.rs`: `passages_for(article, embedder) -> anyhow::Result<Vec<SourceDocument-with-vector-slot>>` = `chunk(text, 256 − token_count(title), &|s| token_count(s) − 2)`, failing if `token_count(title) ≥ 256`; builds `SourceDocument { external_id: "{id}#{ordinal}", fields: {"title": Text(title), "text": Text("{title}\n\n{passage.text}")}, chunk: Some(ChunkInfo { parent: id, ordinal, byte_range: Some(range) }) }`; the schema constructor `wiki_schema()` = `title` (Text `standard_en`, indexed, boost 2.0) + `text` (Text `standard_en`, indexed, boost 1.0), `dense_fields = ["text"]` (D6); unit test over one article from `chunk_b.json` asserting the `text` field starts with `"{title}\n\n"` and the external ids. Commit **PR 2** here (Phase 3 + T021–T025)
- [X] T026 [US1] Implement `crates/xtriever-cli/src/wiki/build.rs` per contracts/cli.md `build`: verify parquet + JSONL against the manifest first; stream `simple.jsonl` (`BufReader` lines, `serde_json::from_str` per line), apply rules with per-rule counters, `--limit N` stops after N articles read and marks `corpus.json` `"partial": N`; chunk; group passages into shards of 4,096 in order; per shard: cache read or embed (`Embedder::embed(&[text], TextKind::Passage)` one at a time) + cache write; `HybridIndex::add_embedded(&batch)` per shard into `<out>.partial/index`; `commit`; `merge`; then the `verify` phase (T027) in-process; write `corpus.json` (canonical JSON: `serde_json` with sorted keys — use `BTreeMap`-based structs — no whitespace), `wiki-build.json`, `ATTRIBUTION.txt` (data-model texts verbatim); `rename(<out>.partial, <out>)` last; remove a stale `<out>.partial` at start; progress lines per phase and per shard (`shard 117/118 hit` / `embedded 4096 in 385.2 s`); per-phase `Instant` timings into the record; `--load-path buffered|mmap` for the embedder
- [X] T027 [US1] Implement `crates/xtriever-cli/src/wiki/verify.rs` per contracts/cli.md `verify`: open the index (`open_with { mapped: true, read_only: true }`), iterate every passage of the store (add `HybridIndex::passage_text(DocId)` if no iteration exists — check `passages.rs` first; prefer an existing accessor), `token_count` each → `max_tokens_seen`, count `> 256` → must be 0; re-derive each article's URL from the title line and compare with the snapshot's `url` (stream the JSONL again, by id) → mismatches must be 0; check `corpus.json` exists and its `counts.passages == index.len()`; print counts; `anyhow::bail!` naming the first offending passage id on any violation
- [X] T028 [US1] Implement `crates/xtriever-cli/src/wiki/record.rs`: the `BuildRecord` and `CorpusIdentity` structs per data-model, `corpus_identity()` = sha256 of the canonical JSON of `{snapshot, exclusions, chunker, embedder_fingerprint}` (sorted keys, no whitespace — one helper `canonical_json(&Value)` used by both files and unit-tested against a Python `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=False)` string embedded in the test); `artefact_bytes` per file; `host` from `std::env::consts` + `MiniLmEmbedder::thread_count()`
- [X] T029 [US1] Implement `crates/xtriever-cli/src/wiki/expected.rs` per contracts/cli.md `expected`: open with both models (`set_reranker`), for each query in `queries.json` and each depth in `[0, 5, 20]` search `k = 10, explain = true`, emit the same JSON shape `xtriever-ffi/examples/fixture_index.rs --scifact` emits (read that file and match it field for field, hex score bits included) so `DeviceMeasurementTests` needs no new decoder
- [X] T030 [US1] Wire `crates/xtriever-cli/src/main.rs` to the three handlers; `--help` shows the contract's flags; `./scripts/check-no-stubs.sh` PASS again; `cargo nextest run -p xtriever-cli` green; clippy `-D warnings` on the crate
- [X] T031 [US1] Development build (quickstart Step 3): `--limit 2000` into `target/xt-wiki-dev` with `--cache-dir target/xt-wiki-cache`; then `verify` standalone; then build again → every shard a hit, `corpus.json` byte-identical (SC-001 at small scale); then start a third build into a fresh `--out`, Ctrl-C during embedding, confirm no `<out>` exists and `<out>.partial` is removed by the next start, and the re-run hits the finished shards. Record the counts and timings in the PR description. ⛔ `passages_over_window > 0` or `url_mismatches > 0` is a report on the chunker contract, not a threshold
- [X] T032 [US1] Open `target/xt-wiki-dev/index` through the 007 Rust surface (`IndexHandle::open` — a small `#[ignore]` test in `crates/xtriever-ffi/tests/readonly.rs` or a one-off `cargo run --example`): info reports the passage count and both identities; a search returns a `Hit` whose `text` starts with a title line and whose `chunk` is populated — the format is unchanged and the surface reads it (spec US1 scenario 5)
- [X] T033 [US1] Run `xtriever wiki expected` on the dev index for the 20 queries under `RAYON_NUM_THREADS=1`, twice; `diff` the two outputs (empty)
- [ ] T034 [US1] Commit **PR 3** (T026–T033). Then start the **full build** (quickstart Step 4) under `RAYON_NUM_THREADS=4` (004 F-005: bit-identical across thread counts, ~94 ms/passage at 4 vs ~140 ms at 1) with `tee target/xt-wiki-build.log`, unattended; it may be resumed across sessions. When it finishes: `verify` standalone; `expected` for the 20 queries; a second `build` run reporting all shards hit and a byte-identical `corpus.json`; copy `wiki-build.json` to `specs/008-wiki-corpus/build-record.json`. ⛔ Any verify failure is a report

---

## Phase 5: User Story 3 — The index ships with the app (Priority: P2)

**Goal**: The read-only open so the bundle is opened in place; the Swift conveniences; staging
with attribution under the bundle budget.

**Independent Test**: `WikipediaTests` green on the simulator against the staged dev or full
index; staged size printed and gated; the bundle untouched after open + search.

- [X] T035 [US3] Implement `crates/xtriever-lexical/src/readonly.rs`: `pub(crate) struct ReadOnlyDirectory { inner: MmapDirectory }` implementing `tantivy::Directory` (`Clone` derive gives `DirectoryClone` via tantivy's blanket impl — cite `directory/directory.rs:243`): `get_file_handle`, `exists`, `atomic_read`, `watch`, `sync_directory` delegate; `open_write`, `delete`, `atomic_write` return `io::Error::new(ErrorKind::PermissionDenied, "read-only index")` mapped into the trait's error types; `acquire_lock` returns `Ok(DirectoryLock::from(Box::new(())))` with a comment citing `reader/mod.rs:194` and why the lock guards nothing here (D11); `Debug` impl; `TantivyIndex::open_read_only(dir)` in `index.rs` = `Index::open(ReadOnlyDirectory::new(MmapDirectory::open(dir)?))` + `manual_reader`, with `writer()` refused up-front with `Error::Io("read-only index")` so the message is ours, not tantivy's lock error; T012 green; if T021 stubbed `open_read_only`, replace the stub now and re-run T013
- [X] T036 [US3] `crates/xtriever-ffi/src/index.rs`: `open` uses `HybridIndex::open_with(dir, embedder, OpenOptions { mapped: matches!(load_path, Mmap), read_only: true })`; remove the "must be writable"/lock-file paragraphs from `ffi/mod.rs`, `index.rs`, `lib.rs` and `specs/007-ffi-surface/contracts/ffi-surface.md` (replace with "opens read-only, including the directory — 008 D11"); `tests/readonly.rs` gains `open_and_search_a_read_only_directory` (`0o555`, model-backed); add a dated note to `specs/007-ffi-surface/report.md` F-001: "resolved by 008 (read-only open, D11)"
- [X] T037 [US3] `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`: delete `writableCopy(of:named:)` and `copyLock`; delete `Tests/XtrieverTests/WritableCopyTests.swift`; `Support.openFixture` and `DeviceMeasurementTests` open the bundled directories directly; add `public extension Hit { var titleAndPassage: (title: String, passage: String)? }` (split at the first `"\n\n"`, `nil` if absent) and `var wikipediaURL: URL?` (D6's rule in Swift: percent-encode UTF-8 bytes outside `A-Za-z0-9-_.~/` as upper-case `%XX`, then `URL(string:)`); `HarnessResources.wikipediaIndexDirectory / wikipediaQueries / wikipediaExpected / wikipediaIsBundled` under `XtrieverData/wikipedia/`; update the class doc (no lock-file caveat, opened in place; 007 F-001 resolved by 008); `README.md` accordingly
- [X] T038 [US3] `scripts/build-ios-package.sh`: `--with-wiki` per contracts/artefact.md "Staging" — requires `target/xt-wiki/index/xtriever-pipeline.json` (prints the build command if absent), copies `index/`, `wiki-build.json`, `ATTRIBUTION.txt`, `reference/fixtures/008/queries.json` → `queries.json`, `target/xt-wiki/expected.json` into `XtrieverData/wikipedia/`, prints `du -sh` of `XtrieverData`, **fails (exit 1) if `du -sk XtrieverData` exceeds 2,000,000,000 bytes** with the measured size; usage comment updated; a `--with-wiki-dev` variant pointing at `target/xt-wiki-dev` is allowed for simulator work and says so in its output
- [X] T039 [US3] Simulator run (quickstart Step 5): `scripts/build-ios-package.sh --with-models --with-fixtures --with-wiki-dev`; the full 007 suite minus `WritableCopyTests` green; `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia … -only-testing:XtrieverTests/WikipediaTests` green — opened in place, bundle untouched, title/passage/URL assertions hold. Commit **PR 4** (Phase 5). ⛔ A bundle-budget failure with the full index is a report with the measured size, not an edited number

---

## Phase 6: User Story 4 — The corpus is measured on a device (Priority: P3)

**Goal**: The 007 harness over the Wikipedia index: footprint against 600 MB, open time,
in-place open, latency per depth, parity — recorded verbatim.

**Independent Test**: a run record from the iPhone with `corpus: "wikipedia"`, a footprint
verdict, and the parity counts.

- [X] T040 [US4] `swift/Xtriever/Tests/XtrieverTests/DeviceMeasurementTests.swift`: read `TEST_RUNNER_XTRIEVER_CORPUS` (`scifact` default, `wikipedia`) and resolve `XtrieverData/<corpus>/{index, queries.json, expected.json}` through `HarnessResources`; open in place (no copy; `openedInPlace: true`, `firstLaunchCopyMs: null`); the record gains `corpus`, `index.bytes` (sum of the index directory's file sizes), `openedInPlace`, `firstLaunchCopyMs`; `ceilingBytes` stays 600 MB; the parity code is unchanged (matched by id, one-sided scores fail); `scripts/extract-device-run.py` unchanged (verify it still extracts)
- [ ] T041 [US4] Device runs (manual, quickstart Step 7): `scripts/build-ios-package.sh --with-models --with-wiki --app`; run `DeviceMeasurementTests` with `TEST_RUNNER_XTRIEVER_CORPUS=wikipedia` once under the `RAYON_NUM_THREADS=1` scheme (mapped) and once under the default-threads scheme (mapped); extract each to `specs/008-wiki-corpus/runs/<device>-<timestamp>-wikipedia-<loadpath>-threads<N>.json`. Record install size and open time. ⛔ Footprint over 600 MB → stop and report with the mapped breakdown (models / index / transient) and the depth-0 latency; a parity miss beyond tolerance → report the pair. The ceiling is not moved by this feature

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T042 [P] BEIR re-run (quickstart Step 6, Rule 5): `hybrid-rerank-v1` on SciFact, NFCorpus, FiQA with the cached embeddings and the committed indexes; `diff` the metrics against `specs/006-rerank-stage/baselines/*.json` — all three empty. ⛔ Any delta is a report (the read-only directory and `merge` touch no scoring; a delta means they do)
- [ ] T043 [P] Write `specs/008-wiki-corpus/report.md`: verdict; the build record summarised (articles, exclusions per rule, passages, phases in hours/minutes, cache hits, artefact bytes per file); the **estimated vs measured** table (D4/D9 estimates: ~480k passages, ~1.5 GB, ~12.5 h — against reality); simulator results; the device table (footprint verdict vs 600 MB, open time, in place, latency mean/p50/max per depth, per-pair cost beside 007's, parity); BEIR deltas (zero); 007 F-001 resolved; the bundle budget vs measured; **the FR-013 statement: this corpus's retrieval quality is unmeasured (owner decision Q2)**; findings
- [ ] T044 [P] Write `specs/008-wiki-corpus/pr-description.md`: summary; five-PR split with line counts; the build record headline; the device table; BEIR deltas (zero, pasted); the list of files changed under `crates/xtriever-{lexical,dense,pipeline,ffi}` with one line each on why (SC-006); no ADR, no format change, no core trait change, no new `unsafe`; CI unchanged except the chunker fixtures run in the workspace test job (no fetch, no build — standing rule)
- [ ] T045 Run the full gate (quickstart Step 8): fmt; clippy `-D warnings` workspace and `-p xtriever-ffi --features cli`; `cargo nextest run --workspace`; `cargo nextest run -p xtriever-ffi -p xtriever-dense --release --run-ignored only`; `cargo deny check`; the three mobile `cargo check`s and wasm best-effort; `check-no-stubs`, `check-containment`, `check-toolchain`; `git diff --stat main -- crates/xtriever-core deny.toml` empty; `git diff --stat main -- crates/xtriever-lexical crates/xtriever-dense crates/xtriever-pipeline crates/xtriever-ffi` lists only `readonly.rs`, `index.rs` (lexical); `embedder.rs` (dense); `index.rs` (pipeline) + tests; `index.rs`, `ffi/mod.rs`, `lib.rs`, `tests/readonly.rs` (ffi); the simulator suite once more; `scripts/fetch-wiki.sh` idempotent. Paste into `report.md` and `pr-description.md`. Commit **PR 5**. ⛔ Any failure — stop and report

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T004 → T005; T002 ‖ T003 ‖ T006 ‖ T007 (T003 needs T002's venv to run, not to write)
- **Red suite (Phase 2)**: T008 (needs T005's JSONL for set B) → T009; T010 ‖ T011 ‖ T012 ‖ T013 ‖ T014 ‖ T015 ‖ T016 after T006/T007; T017 last → **PR 1**
- **US2 (Phase 3)**: T018 ‖ T019 → T020
- **US1 (Phase 4)**: T035 (lexical read-only) ideally **before** T021 — if the PR order keeps T035 in PR 4, T021 stubs `open_read_only` and T035 replaces it; T021 → T022; T023 ‖ T024 → T025 → **PR 2**; T026 → T027 → T028 → T029 → T030 → T031 → T032 → T033 → T034 (**PR 3**, then the ~12 h build)
- **US3 (Phase 5)**: T035 → T036 → T037 → T038 → T039 → **PR 4** (can proceed while the full build runs, using the dev index)
- **US4 (Phase 6)**: T040 → T041 (needs the full build and PR 4)
- **Polish (Phase 7)**: T042 ‖ T043 ‖ T044 → T045 → **PR 5**

### Rule 6 stop-points (⛔)

T020 (golden mismatch); T031 / T034 (a passage over the window, a URL mismatch, a verify
failure); T039 (bundle over budget); T041 (footprint over 600 MB, parity miss); T042 (any BEIR
delta); T045 (any gate failure). The response is a report, never a looser bound, a wider
tolerance, an edited fixture or a raised number.

### Parallel Opportunities

- Phase 1: T002 ‖ T003 ‖ T006 ‖ T007
- Phase 2: T010 ‖ T011 ‖ T012 ‖ T013 ‖ T014 ‖ T015 ‖ T016 (seven files/trees)
- Phase 3: T018 ‖ T019
- Phase 4: T023 ‖ T024
- Phase 5 runs alongside the full build started in T034
- Phase 7: T042 ‖ T043 ‖ T044

---

## Parallel Example: Phase 2 red suite

```text
after T005 (snapshot pinned) and T006/T007 (scaffolds):
  T008 fixtures (Python)   → T009 goldens (Rust)
  T010 properties   T011 token_count   T012 lexical read-only   T013 pipeline open_with/merge
  T014 cli units    T015 queries       T016 Swift WikipediaTests
then T017 (red checkpoint, PR 1)
```

## Implementation Strategy

1. **PR 1** (Phases 1–2): the snapshot is pinned and fetchable, every oracle exists and is red.
2. **PR 2** (Phase 3 + T021–T025) — **MVP**: the chunker with byte-exact goldens, the
   embedder's window count, the pipeline's `open_with`/`merge`, and the CLI's pure parts
   (manifest, rules, URL, cache, document shaping). Nothing builds yet, but everything that
   decides what the index contains is proven.
3. **PR 3** (T026–T034): the build, verify and expected commands; the development build proves
   resumability and reproducibility; the full build starts.
4. **PR 4** (Phase 5): read-only open end to end, Swift conveniences, staging under budget,
   simulator green — done while the full build runs.
5. **PR 5** (Phases 6–7): the build record, the device records, the report, the BEIR zero-delta,
   the gate.
