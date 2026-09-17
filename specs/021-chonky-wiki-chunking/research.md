# Research: Chonky Chunking for the Wikipedia Demo Build

Read on 2026-09-17: `chonky` 0.1.7 (`src/chonky/__init__.py` at `main`), its README, the
pinned model's `config.json` / `tokenizer_config.json` at revision `01d8aae…`; the demo's
`build.py`, `chunking.py`, `inputs.py`, `about.py`, `record.py` and tests; the experiment on
60 snapshot articles (spec input).

## D1 — What `chonky` does, exactly

`ParagraphSplitter(model_id=…, device="cpu")` loads `AutoTokenizer` and
`AutoModelForTokenClassification` (`transformers`) and builds a `"ner"` pipeline with
`aggregation_strategy="simple"` and `stride = tokenizer.model_max_length // 2` (512 → 256 for
this model — long texts are processed in overlapping windows). `__call__(text)` yields
`text[begin:ner["end"]]` for each predicted `separator` entity and then the tail: **contiguous
character slices whose concatenation is `text`** (asserted on all 60 articles). A chunk may
begin with the whitespace that followed the previous separator (`"\n\n…"`).

The library's default `model_id` (`mirth/chonky_distilbert_uncased_1`) is not the README's
(`…_base_uncased_1`); the demo always passes a local directory, so the default is never used.

## D2 — The model, pinned

`reference/models/manifest-chonky.json`: `repository` `mirth/chonky_distilbert_base_uncased_1`,
`revision` `01d8aae08726368a1b1645de2a7086610f2e86a5`, `local_dir`
`chonky_distilbert_base_uncased_1`, `files`: `config.json`, `model.safetensors`,
`special_tokens_map.json`, `tokenizer.json`, `tokenizer_config.json`, `vocab.txt` — the six
files `from_pretrained` reads (`.gitattributes` and `README.md` are not fetched). Sizes and
sha256 are measured from the files at that revision (already in the local HF cache from the
experiment; `scripts/fetch-model.sh --manifest …` downloads and verifies them into
`reference/models/chonky_distilbert_base_uncased_1`). The script reads its file list from
the manifest, so it needs no change. The model: `DistilBertForTokenClassification`, 512
positions, labels `O` / `separator`, uncased.

## D3 — The chunker module (rewritten, < 80 lines)

`wikidemo/chunking.py`:

- `class Splitter`: `__init__(model_dir)` → `ParagraphSplitter(model_id=str(model_dir),
  device="cpu")` (imported lazily so `search` / `about` never import `torch`);
  `chunks(text) -> list[tuple[int, int]]`: the splitter's slices as character ranges,
  reconstructed by walking the yielded chunks (`start += len(chunk)`), with an assertion that
  the walk ends at `len(text)` (the partition, checked at build time — a failure is a
  `BuildError`, spec SC-001).
- `class Window`: `token_count(s)` with the embedder's `tokenizer.json` (`tokenizers`, no
  truncation / padding — as before) and `WINDOW = 256`; used only for
  `passages_over_window`.
- `documents_for(article, splitter, window) -> tuple[list[Document], int]`: for each
  `(start, end)` char range, `chunk = text[start:end]`; skip if `chunk.strip()` is empty;
  `byte_start = len(text[:start].encode())`, `byte_end = byte_start + len(chunk.encode())`
  (computed incrementally); `Document(external_id=f"{id}#{ordinal}", fields={"title",
  "text": passage_text(title, chunk.strip())}, chunk=ChunkInfo(parent, ordinal, byte_start,
  byte_end))`; `over += token_count(passage_text) > WINDOW`. Ordinals count emitted passages.
- `passage_text`, `BuildError` as before. `WHITESPACE`, `paragraphs`, `sentences`, `words`,
  `fragments`, `chunk`, `Pricer` — deleted.

## D4 — The over-window trade-off

The engine's `MiniLmEmbedder` truncates and pads to 256 positions (`embedder.rs:106-137`):
a passage longer than the window is embedded from its first 256 word-pieces; the lexical
index sees all of it. Measured on 60 articles: 29 of 302 chunks over the window (~10 %),
median 77 tokens, p90 248, max 4,745. The count goes to `counts.passages_over_window`
(the field already exists in the 008 shape — always 0 there), `about` prints it, the
record and the README state the share. No fallback split (owner's decision).

## D5 — The sidecar's chunker block and the identity

`record.CHUNKER = {"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1",
"revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}`; the identity hashes it as before
(`corpus_identity(snapshot, exclusions, chunker, fingerprint, partial)`), so a chonky-built
index has a different identity from the Rust build's at every N — as it should: different
passages. `about` prints `chunker: chonky (mirth/…, revision 01d8aae…)` for this block and
`chunker: 008 contract (256 - token_count(title))` for the shipped artefact's `{version,
budget, cost}` block. `test_record.test_identity_of_the_shipped_corpus` keeps the 008 block
as a literal (the function is unchanged); the `CHUNKER` assertion moves to the new block.

## D6 — Inputs

`inputs.py`: a `chonky` input — default `reference/models/chonky_distilbert_base_uncased_1`,
env `XTRIEVER_CHONKY_MODEL_DIR`, flag `--chonky`, sentinel `model.safetensors`, producer
`scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json`; `build` needs it
(`needs_for`). The conftest's `missing_for_models` adds it so the model-backed build tests
skip with the reason when it is absent.

## D7 — Dependencies

`pyproject.toml`: `chonky==0.1.7`, `transformers==5.17.0`, `torch==2.14.0` (the versions the
resolver installed into `reference/.venv-012` on 2026-09-17; `transformers` 5.17 works with
the pinned `tokenizers==0.23.2`, verified there), `tokenizers==0.23.2` kept for the
over-window count. The demo venv is re-installed (`uv pip install -e ".[test]"`; torch is
the heavy one). `torch` is imported only by `build` (lazy import in `Splitter`).

## D8 — Tests

`test_chunking.py` rewritten: model-free — `Splitter.chunks` offset arithmetic on a stub
splitter (a fake `ParagraphSplitter` yielding chosen slices, including multi-byte characters
and a whitespace-only chunk) → byte ranges slice back to the chunks, empty chunk skipped,
ordinals contiguous; `documents_for` shapes; the over-window count with a stub window;
`models` — the real splitter on the three synthetic articles and one long real-shaped text
(~20k characters of prose with paragraphs): the chunks concatenate to the text, every
byte range slices back. `test_build.py`: the chunker block assertion → the chonky block;
`passages_over_window` present. `test_record.py`: `CHUNKER` → chonky block. `test_cli`,
`test_inputs`: the new input and producer. `conftest`: `CHONKY` path in the skip.

## D9 — The slice and the record

`wikidemo build --limit 2000 --out target/xt-wiki-slice-chonky`; `search` / `about` on it;
the run record `specs/021-chonky-wiki-chunking/runs/slice-chonky-<machine>-<stamp>.json` =
the artefact's `wiki-build.json` (counts, phases including the split's share of `chunk`,
host by model name) plus a `split` block (chunks per article median/max, passage token
median/p90/max, over-window share) computed by a small `--stats` pass? No — the build itself
records `counts.passages_over_window`; the token statistics come from the same `Window`
during the build and go into `wiki-build.json` under `chunking: {passages, over_window,
token_median, token_p90, token_max}` (one `statistics` call). The record is that file,
copied under `runs/` with the machine name from `record.machine_name()` (never a hostname).
The old `slice-…` record (Rust parity) stays as history; the README stops describing it.

## D10 — Not done

No fallback split for over-window chunks; no change to the minimal demo; no change to the
Rust build, the fixtures or the shipped artefact; no CI job; `measure --against` stays in the
code (valid between two chonky builds) but leaves the README.
