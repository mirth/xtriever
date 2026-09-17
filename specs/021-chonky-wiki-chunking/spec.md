# Feature Specification: Chonky Chunking for the Wikipedia Demo Build

**Feature Branch**: `021-chonky-wiki-chunking`

**Created**: 2026-09-17

**Status**: Draft

**Input**: User description: "Chonky chunking for the Wikipedia demo build: `wikidemo build` (apps/python-wiki-demo) splits each article into passages with the `chonky` library (https://github.com/mirth/chonky, `ParagraphSplitter`, a fine-tuned token-classification model that yields contiguous slices of the original text at predicted semantic breaks) instead of the Feature 008 contract chunker, and the demo's copy of that chunker (`wikidemo/chunking.py`'s chunk/paragraphs/sentences/words/fragments, the `Pricer`, the fixture replay tests) is removed — the recipe becomes "split with chonky, add, commit, merge". The model is `mirth/chonky_distilbert_base_uncased_1`, pinned like the other models: a `reference/models/manifest-chonky.json` (revision 01d8aae08726368a1b1645de2a7086610f2e86a5, its files with sizes and sha256) fetched by the existing `scripts/fetch-model.sh` into `reference/models/chonky_distilbert_base_uncased_1` and loaded from that directory (`ParagraphSplitter(model_id=<dir>, device="cpu")`), never from the hub at run time. Each chonky chunk becomes one passage: stripped of surrounding whitespace for the stored text (`<title>\n\n<chunk>`), with `ChunkInfo` byte offsets of the chunk inside the article's text (the chunks partition the text — a test asserts the concatenation equals the article); empty chunks are skipped. Chunks longer than the embedder's 256-token window are embedded as they are (the engine embeds the first 256 word-pieces; BM25 indexes the whole chunk) — the build counts them (`passages_over_window`) and the report states the count and the trade-off; measured on 60 articles: median 77 tokens, p90 248, about 10 % over the window, max 4,745; chonky splits at ~60k characters per second on this laptop (the 2,000-article slice in ~20 s). The sidecar's `chunker` block records `{"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae…"}` and the corpus identity changes accordingly (a demo-built index is no longer the Rust build's — `measure --against` the Rust slice is dropped from the docs and its record; the host-goldens `measure` over the shipped artefact is unchanged). The 2,000-article slice is rebuilt with chonky and searched; `wikidemo search` and `about` work on it unchanged; a run record with the split's chunk-count and over-window statistics and the build time is committed. The exclusion rules, the URL check, the sidecars, the staging rename, `tokenizers` for `passages_over_window` counting only (or dropped if not needed), the minimal demo (apps/python-minimal-demo — untouched: its documents are passage-sized), the engine, FFI, format and baselines are unchanged; the demo's dependencies gain `chonky` (and with it `transformers` and `torch`) at versions read from the resolver, pinned in the demo's manifest; tests first: the split-is-a-partition and offsets test, the sidecar/identity test updated, the synthetic-snapshot build test through chonky, the fixture-replay tests removed; CI runs nothing that needs any model. README: the recipe section rewritten around chonky, the 008 schema note kept, the 013 schema note kept, the "same index as the Rust build" claim removed with the reason."

**Owner's decisions (2026-09-17, before this spec)**: the Wikipedia demo's build only (the
minimal demo stays dependency-free); chunks over the embedder's window are embedded as they
are and counted; the model is pinned and fetched like the engine's models.

## Why This Spec Reads Technically

The Wikipedia demo's build carried a 250-line copy of the engine's chunking contract —
paragraphs, sentences, words, fragments, priced by the embedder's tokenizer — because the
Rust build was its oracle. That made the demo *provably the same* as the shipped index and
*unreadable as a recipe*: nobody assembling their own RAG system will re-implement a
tokenizer-priced splitter. A semantic splitter that reads the text and says where the
breaks are is what such a person actually reaches for, and the owner's `chonky` library is
one: a small fine-tuned model that returns the article as contiguous slices at predicted
paragraph breaks. This feature swaps the contract chunker for it and accepts what that
costs — the demo build stops being byte-identical to the Rust build, so that oracle goes,
and about a tenth of the chunks run past the embedder's 256-token window and are embedded
from their head. What stays measured: the split is a partition of the text (tested), the
model is pinned by revision and hash like the others, the over-window count is on the
record, and search over the rebuilt slice still works unchanged.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The build splits articles with chonky (Priority: P1)

`wikidemo build --limit N --out DIR` reads the articles, drops the excluded ones, splits
each with the semantic splitter, adds every non-empty chunk as one passage with its
provenance (the article id, the chunk's position, its byte range in the article), commits,
merges and writes the sidecars as before. The stored passage is the title line, a blank
line, the chunk stripped of surrounding whitespace.

**Why this priority**: This is the feature.

**Independent Test**: A synthetic three-article snapshot builds; every chunk's byte range
slices the article's text back to the chunk; the chunks of an article concatenate to its
text; the 2,000-article slice builds and `search` / `about` work on it.

**Acceptance Scenarios**:

1. **Given** an article, **When** split, **Then** the chunks concatenate to exactly the
   article's text, and each passage's byte range selects its chunk from the text.
2. **Given** a chunk that is only whitespace, **When** split, **Then** it produces no
   passage and the following ordinals close the gap (ordinals count passages, not chunks).
3. **Given** a chunk longer than the embedder's window, **When** added, **Then** it is
   indexed whole for lexical search and embedded from its head, and the build's
   `passages_over_window` count includes it.
4. **Given** the 2,000-article slice, **When** built with chonky, **Then** `wikidemo search
   --artefact DIR "April"` returns that article's passages and `about` shows the sidecar
   with the chonky chunker block and the over-window count.

---

### User Story 2 - The model is pinned and fetched like the others (Priority: P1)

The splitter's model is listed in a manifest with its repository, revision and every
file's size and hash; the existing fetch script downloads and verifies it into
`reference/models/`; the build loads it from that directory and never contacts the hub.

**Why this priority**: Principle IV — reproducible builds; the standing rule that models
are pinned by SHA.

**Independent Test**: `scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json`
reports PASS; a build with the network off succeeds; a missing model directory is reported
as a missing input with the fetch command.

---

### User Story 3 - The recipe reads as one (Priority: P2)

The demo's chunking module is the splitter call and the document shaping — no
paragraphs / sentences / words / fragments, no pricing; the README's recipe section names
the splitter, the model, the over-window trade-off with the measured numbers, and no longer
claims the demo's index equals the Rust build's (it says why it does not).

**Independent Test**: the chunker module is under 80 lines; the fixture-replay tests are
gone; the README section and the `about` output agree on the chunker.

---

### Edge Cases

- An article whose text is empty or only whitespace: zero passages; counted as selected.
- An article longer than the splitter's own window: the splitter handles it in strides
  (the library's behaviour); the partition test covers a long article.
- The chunker block in the sidecar changes the corpus identity: an index built by this
  feature has a different identity from the shipped one at every N, and `about` labels the
  chunker so the difference is visible.
- `measure --against` two chonky-built slices still works (same recipe, same result);
  against the Rust slice it would FAIL — the README no longer suggests it.
- The engine, the shipped artefact and the phone are untouched; `measure` over the shipped
  artefact against the host goldens is unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `wikidemo build` MUST split each selected article with the `chonky`
  semantic splitter loaded from the pinned local model directory, never from the hub at
  run time; the model directory MUST be a checked input (a missing one is reported with the
  fetch command before anything loads).
- **FR-002**: The model MUST be pinned in `reference/models/manifest-chonky.json`
  (repository `mirth/chonky_distilbert_base_uncased_1`, revision
  `01d8aae08726368a1b1645de2a7086610f2e86a5`, every file with its size and sha256) and
  fetched by the existing `scripts/fetch-model.sh` into
  `reference/models/chonky_distilbert_base_uncased_1`.
- **FR-003**: Each non-empty chunk MUST become one passage: `external_id = "<article
  id>#<ordinal>"` with ordinals counting passages, `text = "<title>\n\n<chunk stripped>"`,
  `title = <title>`, `ChunkInfo(parent, ordinal, byte_start, byte_end)` where the byte
  range is the chunk's (unstripped) span in the article's UTF-8 text; the chunks of an
  article MUST concatenate to its text exactly.
- **FR-004**: Chunks whose `"<title>\n\n<chunk>"` exceeds 256 embedder positions MUST be
  added unchanged and counted in `counts.passages_over_window`; the count MUST appear in
  `about` and in the build's output.
- **FR-005**: The sidecar's `chunker` block MUST be `{"name": "chonky", "model":
  "mirth/chonky_distilbert_base_uncased_1", "revision": "<the pinned revision>"}`; the
  corpus identity MUST be computed over it as before (so it differs from the Rust build's).
- **FR-006**: The demo's chunker copy (paragraphs / sentences / words / fragments, the
  pricer) and its fixture-replay tests MUST be removed; the exclusion rules, the URL check,
  the sidecars, the staging rename, the build record and the progress output MUST be
  unchanged in behaviour.
- **FR-007**: The demo's dependencies MUST gain `chonky` (with `transformers` and `torch`
  as its own dependencies) at versions read from the resolver and pinned in the demo's
  manifest; `tokenizers` stays only if the over-window count needs it.
- **FR-008**: Tests first, committed failing: the split-is-a-partition and byte-offset
  test (on the synthetic articles and on one long real article shape), the
  document-shaping test, the sidecar/identity test with the new chunker block, the
  synthetic-snapshot build test through chonky (models), the over-window count test.
- **FR-009**: The 2,000-article slice MUST be rebuilt with chonky, searched, and a run
  record committed with the passage count, the over-window count, the split time and the
  build time; the previous Rust-slice parity record stays as history and the README no
  longer describes `measure --against` the Rust build.
- **FR-010**: No change under `crates/`, `swift/`, `python/src`, `apps/python-minimal-demo`,
  `reference/fixtures`, any baseline or the shipped artefact; the host-goldens `measure`
  unchanged; CI runs nothing that needs a model.

### Key Entities

- **Chunk**: a contiguous slice of the article's text from the splitter; `(start, end)`
  character offsets → byte offsets; stripped for the stored passage.
- **Passage**: as before (id, title, text, ChunkInfo).
- **Counts**: `articles`, `excluded`, `selected`, `passages`, `passages_over_window`,
  `url_mismatches` — the over-window count now non-zero by design.
- **Manifest (chonky)**: repository, revision, local_dir, files[{name, bytes, sha256}].

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For every article in the tests and in the slice build, the chunks
  concatenate to the text and every byte range slices back to its chunk (asserted in the
  tests; counted in the build — a mismatch stops the build).
- **SC-002**: The 2,000-article slice builds with chonky; the split takes under one minute
  and the whole build under 30 minutes on this laptop; the record states the passage
  count and the over-window share (expected around 10 %).
- **SC-003**: `scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json`
  PASSES; a build with a missing model directory is refused with the fetch command in
  under a second.
- **SC-004**: The demo's chunking module is under 80 lines; the suite has no fixture
  replay; `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/fixtures specs/*/baselines` is empty.

## Assumptions

- **The engine embeds over-long passages from their head** (its embedder truncates to
  256 positions) and BM25 indexes them whole — the trade-off the owner accepted.
- **The over-window count needs the embedder's tokenizer** (`tokenizers`, already a
  dependency) — kept for that purpose only.
- **The splitter runs on the CPU** (`device="cpu"`); ~60k characters per second here.
- **`chonky`'s default `model_id` differs from the README's** (`chonky_distilbert_uncased_1`
  in the code vs `chonky_distilbert_base_uncased_1` on the hub) — the demo always passes the
  local directory, so the default is never used.
- **No branch or commit by the agent**; the owner creates `021-chonky-wiki-chunking` and
  commits at the checkpoints.
