# Data Model: Chonky as an Optional Chunker for the Wikipedia Demo Build

## Chunker (build-time choice)

| Field | Type | Notes |
|---|---|---|
| name | `"contract"` \| `"chonky"` | `--chunker`; default `contract` |
| documents_for | `article → (documents, positions)` | one `xtriever.Document` per passage; the position count of each stored passage (`Window.token_count`) |
| block | dict | the sidecar's `chunker` block (below) |
| label | str | the completion line / `about` line |

**ContractChunker** (`contract.py`): needs the embedder's tokenizer (`Window`); budget
`256 − token_count(title)`, cost `token_count(unit) − 2`; title ≥ 256 positions → `BuildError`.
**ChonkyChunker** (`chonky_chunker.py`): needs the pinned model directory and the extra;
the slices must partition the text → else `BuildError`.

## Chunker block (sidecar `index/corpus.json` → `chunker`, in the corpus identity)

| Chunker | Block |
|---|---|
| contract | `{"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}` — the shipped artefact's literal strings |
| chonky | `{"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}` |

`corpus_identity = sha256(canonical_json({snapshot, exclusions, chunker, embedder_fingerprint[, partial]}))`
— unchanged; a contract slice of the first 2,000 articles therefore has the Rust slice's
identity `20949fb4…`.

## Passage, Counts, Record

As Feature 021: `external_id = "<article id>#<ordinal>"`, fields `title` / `text`
(`"<title>\n\n<passage>"`), `ChunkInfo(parent, ordinal, byte_start, byte_end)`;
`counts.{articles, excluded, selected, passages, passages_over_window, url_mismatches}`;
`wiki-build.json.chunking.{passages, over_window, token_median, token_p90, token_max}` and
`phases_ms.load_splitter` (the tokenizer load for contract, the model load for chonky).

## Inputs per build

| Input | contract | chonky |
|---|---|---|
| embedder, re-ranker, snapshot, manifest | required | required |
| chonky model directory (`--chonky`, `XTRIEVER_CHONKY_MODEL_DIR`) | not checked | required (fetch command on absence) |
| the `chonky` extra (`chonky`, `transformers`, `torch`) | not imported | required (install command on absence) |

## Records (this feature's `runs/`)

- `slice-contract-<machine>-<stamp>.json`: the default 2,000-article build's `wiki-build.json`.
- `parity-<machine>-<stamp>.json`: `measure --against` the Rust slice; `against.verdict`
  and `parity.verdict` both PASS.
- `slice-chonky-200-<machine>-<stamp>.json`: the 200-article chonky build's `wiki-build.json`.
