# Contract: `wikidemo build` with chonky

```
wikidemo build --out DIR [--limit N] [--snapshot FILE] [--manifest FILE] [--chonky DIR]
```

Inputs checked before anything loads (in this order): the embedder, the re-ranker, the
snapshot, the manifest, **the chonky model directory** (`--chonky`, `XTRIEVER_CHONKY_MODEL_DIR`,
default `reference/models/chonky_distilbert_base_uncased_1`; missing →
`wikidemo: missing the chonky splitter model: <path>/model.safetensors` /
`produce it with: scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json`,
exit 1).

Behaviour as Feature 019's contract except:

- each selected article is split by the chonky splitter; every non-empty chunk is one
  passage (`contracts` of 019 for ids, fields, provenance); the chunks must concatenate to
  the article's text — otherwise `wikidemo: article <id>: the splitter did not return a
  partition of the text`, exit 1, nothing at `<out>`;
- the completion output gains `passages over the embedder window: N (P %)` and
  `chunker: chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae…)`;
- `index/corpus.json`'s `chunker` block is the chonky block; `counts.passages_over_window`
  is the count; `wiki-build.json` gains `chunking: {passages, over_window, token_median,
  token_p90, token_max}`.

`wikidemo about` prints, after the counts line, `passages over the embedder window: N` and
`chunker: chonky (…)` — or `chunker: 008 contract (256 - token_count(title))` for an
artefact built by the Rust CLI.

`search`, `measure` (host goldens) unchanged. `measure --against` compares any two
artefacts as before; the README no longer proposes the Rust slice as the second one.

## `reference/models/manifest-chonky.json`

```json
{
  "schema_version": 1,
  "repository": "mirth/chonky_distilbert_base_uncased_1",
  "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5",
  "local_dir": "chonky_distilbert_base_uncased_1",
  "files": [
    {"name": "config.json", "bytes": 0, "sha256": "…"},
    {"name": "model.safetensors", "bytes": 0, "sha256": "…"},
    {"name": "special_tokens_map.json", "bytes": 0, "sha256": "…"},
    {"name": "tokenizer.json", "bytes": 0, "sha256": "…"},
    {"name": "tokenizer_config.json", "bytes": 0, "sha256": "…"},
    {"name": "vocab.txt", "bytes": 0, "sha256": "…"}
  ],
  "note": "The chonky paragraph splitter (Feature 021): DistilBertForTokenClassification, 512 positions, labels O / separator. Fetched by scripts/fetch-model.sh; loaded by the Wikipedia demo's build from the local directory, never from the hub."
}
```

(sizes and hashes measured at implementation time from the files at that revision.)
