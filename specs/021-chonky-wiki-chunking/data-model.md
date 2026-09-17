# Data Model: Chonky Chunking for the Wikipedia Demo Build

| Entity | Fields | Rule |
|---|---|---|
| **Chunk** | `start`, `end` (character offsets in the article's text) | contiguous, in order, `start` of the first = 0, `end` of the last = `len(text)`; a walk that does not end at `len(text)` is a `BuildError` |
| **Passage** | `external_id = "<id>#<ordinal>"`, `title`, `text = "<title>\n\n<chunk.strip()>"`, `ChunkInfo(parent=id, ordinal, byte_start, byte_end)` | ordinals count emitted passages (whitespace-only chunks emit none); byte range = the unstripped chunk's UTF-8 span |
| **Window** | `WINDOW = 256`; `token_count(s)` with the embedder's tokenizer | `over_window` iff `token_count(text) > 256` |
| **Counts** | `articles`, `excluded{…}`, `selected`, `passages`, `passages_over_window`, `url_mismatches` | unchanged shape; `passages_over_window` now non-zero by design |
| **Chunker block** (sidecar) | `{"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}` | hashed into the corpus identity |
| **Build record** (`wiki-build.json`) | as 019 plus `chunking: {passages, over_window, token_median, token_p90, token_max}` | the committed run record is this file under `runs/` |
| **Chonky manifest** | `schema_version`, `repository`, `revision`, `local_dir`, `files[{name, bytes, sha256}]` | six files; fetched and verified by `scripts/fetch-model.sh` |
| **Input** | `chonky` model dir — flag `--chonky`, env `XTRIEVER_CHONKY_MODEL_DIR`, default `reference/models/chonky_distilbert_base_uncased_1`, sentinel `model.safetensors` | needed by `build` only |
