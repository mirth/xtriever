# Contract: `wikidemo build --chunker`

```
wikidemo build --out DIR [--limit N] [--snapshot FILE] [--manifest FILE]
               [--chunker {contract,chonky}] [--chonky DIR]
```

`--chunker` defaults to `contract`. `build --help` names both chunkers and the default. An
unknown name is a usage error (exit 2, argparse's message with the two choices).

## Before anything loads (in this order)

1. The inputs (`exists` checks): the embedder, the re-ranker, the snapshot, the manifest;
   **and, only for `--chunker chonky`, the chonky model directory** (`--chonky`,
   `XTRIEVER_CHONKY_MODEL_DIR`, default `reference/models/chonky_distilbert_base_uncased_1`).
   The first missing one → `wikidemo: missing <what>: <path>` / `produce it with: <command>`,
   exit 1.
2. For `--chunker chonky`, the extra: if the `chonky` package is not installed →
   `wikidemo: --chunker chonky needs the demo's chonky extra; install it with:
   uv pip install --python apps/python-wiki-demo/.venv/bin/python -e 'apps/python-wiki-demo[chonky]'`,
   exit 1, before the snapshot is read. Decided without importing the package.
3. Then the 019 build: verify, exclude, split, add, commit, merge, sidecars, rename.

A default build never imports `chonky`, `transformers` or `torch`.

## `--chunker contract` (default)

Each selected article is cut by the Feature 008 contract (`specs/008-wiki-corpus/contracts/chunker.md`):
paragraphs → sentences → words → fragments, priced by the embedder's tokenizer at
`token_count(unit) − 2` against `256 − token_count(title)`; a title of 256 or more positions
stops the build (`wikidemo: article <id> (<title>): the title alone needs N positions, the
window is 256`). Passages, ids, fields and provenance are the Rust build's, so for the same
articles the artefact has the Rust build's corpus identity and `measure --against` the Rust
artefact is PASS. Completion output: `chunker: 008 contract (256 - token_count(title))`;
`passages over the embedder window: 0 (0.0 %)`.

## `--chunker chonky`

As Feature 021's contract (`specs/021-chonky-wiki-chunking/contracts/build.md`): the chonky
slices, one passage per non-empty slice, the partition check, over-window passages added
whole and counted; the chonky chunker block; `chunker: chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae…)`.

## Sidecar and record

`index/corpus.json.chunker` is the contract block or the chonky block (data-model.md);
`wiki-build.json` unchanged in shape (`chunking` statistics for both).

## `about`, `search`, `measure`

Unchanged. `about` prints `chunker: 008 contract (…)` or `chunker: chonky (…)`. `measure
--against` compares any two artefacts; a contract slice against the Rust slice of the same
articles is PASS; a chonky slice against either is FAIL on the identity line, as designed.

## Install

```bash
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test]"          # search, about, measure, default build
uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test,chonky]"   # + --chunker chonky
```
