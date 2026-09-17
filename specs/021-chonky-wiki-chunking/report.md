# Report: Chonky Chunking for the Wikipedia Demo Build

**Feature**: 021 · **Branch**: `021-chonky-wiki-chunking` · **Status**: in progress

## The pin (T001)

`reference/models/manifest-chonky.json`: `mirth/chonky_distilbert_base_uncased_1` at
`01d8aae08726368a1b1645de2a7086610f2e86a5`, six files (config 681 B, model.safetensors
265,470,008 B, special_tokens_map 695 B, tokenizer.json 711,494 B, tokenizer_config 1,335 B,
vocab.txt 231,508 B) with sha256 measured from the files at that revision; the
safetensors' hash equals the HF cache's LFS blob name (cross-check).
`scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json` downloaded the six
files from the hub and reported **PASS** — the hub's files match the pin. The directory is
gitignored under `reference/models/`.

## Red checkpoint (2026-09-17)

At checkpoint C1: `test_chunking.py` errors on import (`Splitter`, `Window` do not exist);
`test_build` 2 failed (the chunker block, the `chunking` record), `test_record` 1 failed
(`CHUNKER` is still the 008 block), `test_inputs` 3 failed (no `chonky` input),
`test_cli` 1 failed (the missing-splitter refusal) — 7 failed, 14 passed across the five
files. Dependencies pinned and installed: chonky 0.1.7, transformers 5.17.0, torch 2.14.0,
tokenizers 0.23.2.
