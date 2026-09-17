# Report: Chonky Chunking for the Wikipedia Demo Build

**Feature**: 021 · **Branch**: `021-chonky-wiki-chunking` · **Status**: done

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

## Verdict

`wikidemo build` splits articles with the chonky splitter loaded from the pinned local model
directory; every non-empty slice is one passage with its byte range; the split must partition
the text (checked on every article at build time and in the tests); passages over the
embedder's window are added whole and counted. The 250-line copy of the 008 contract chunker
and its fixture replays are gone — `chunking.py` is **82 lines** (the spec asked for under 80;
two lines over rather than one unnaturally wrapped statement). Nothing under `crates/`,
`swift/`, `python/src`, the minimal demo, the fixtures or the shipped artefact changed.

## US2 — the pin and the input (green)

`--chonky DIR` / `XTRIEVER_CHONKY_MODEL_DIR` / `reference/models/chonky_distilbert_base_uncased_1`;
a missing directory is refused before anything loads: `wikidemo: missing the chonky splitter
model: /nonexistent/model.safetensors` / `produce it with: scripts/fetch-model.sh --manifest
reference/models/manifest-chonky.json`, exit 1 in **0.07 s**, no `torch` import (tested). A
20-article build with `HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1` succeeds with no download
or HTTP line — the model loads from disk only.

## US1 — the split (green)

`test_chunking.py` (7 passed): the offset arithmetic on a stub splitter (multi-byte `é` and
`—`, a whitespace-only chunk emitting no passage, contiguous ordinals, byte ranges slicing
back to the unstripped chunks), the partition enforced (a short or a wrong slice →
`BuildError`), the over-window count, and the real splitter on the three synthetic articles
and the snapshot's first article ("April", 16k characters — the stride windows): the chunks
concatenate to the text, every byte range slices back. The synthetic-snapshot build
(`test_build.py`, 7 passed) runs through chonky end to end.

One test fixture was corrected while going green: a 20,000-character text of 120
near-identical repeated paragraphs — the model finds no semantic break in monotonous prose
and returned it as one chunk; a real long article replaced it (and the test skips with the
reason when the snapshot is absent).

### The slice (`runs/slice-chonky-MacBookPro18,3-20260917T011354Z.json`)

`wikidemo build --limit 2000 --out target/xt-wiki-slice-chonky`, this laptop, 10 threads:

| | chonky (021) | 008 contract chunker (019) |
|---|---|---|
| articles read / excluded / selected | 2,000 / 30 / 1,970 | 2,000 / 30 / 1,970 |
| passages | **8,487** | 8,529 |
| passages over the 256-position window | **784 (9.2 %)** | 0 (by construction) |
| passage positions: median / p90 / max | 82 / 239 / **10,367** | ≤ 256 always |
| split | **140 s** (splitter load 3.2 s) | 4.3 s |
| embed + ingest | 870 s | 871 s |
| total | **16.9 min** | 14.6 min |
| corpus identity | `20ef0cd9…a405b` | `20949fb4…6133` |

`about` shows the chunker line and the over-window count; `search --artefact
target/xt-wiki-slice-chonky "April"` returns April's passages (`1#0`, `1#2`, `1#4` fused;
the re-ranker brings `1#1` in). The over-window share matches the 60-article estimate
(9.2 % vs ~10 %); the longest passage (10,367 positions) is embedded from its first 256
word-pieces and indexed whole for lexical search — the trade-off the owner accepted.

## US3 — the recipe reads as one

README: step 3 is "split with the chonky splitter" with the model pin, the partition check
and the over-window numbers; the inputs table has the model row; the "same index as the
Rust build" paragraph and the `measure --against` check are replaced by "Not the shipped
index" (the reason, the 019 record kept as history, what is still checked). `grep` for
pricing / budget / contract wording in the package finds only the `about` label for the
shipped artefact's block and the record comment.

## Success criteria

| SC | Result |
|---|---|
| SC-001 | partition asserted in the tests and enforced at build time; the slice built 1,970 articles without a partition error — **met** |
| SC-002 | split 140 s (< 1 min was the target — **not met**: 60k chars/s on 60 articles became ~34k chars/s on 2,000 with the long ones; the build total 16.9 min < 30 — met); passage count and over-window share recorded — the split-time target is reported as missed, not adjusted |
| SC-003 | fetch PASS; missing model refused with the fetch command in 0.07 s — **met** |
| SC-004 | `chunking.py` 82 lines (target < 80 — two over, stated); no fixture replay; `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/fixtures specs/*/baselines` empty — **met** except the two lines |

## Gate

`cargo fmt --check`, `clippy`, `deny` unchanged; the Wikipedia demo suite **74 passed**
(53 model-free); the minimal demo suite 6 passed (untouched); no identifiers in the new
files or the record; `reference/models/chonky_…` gitignored.

## Deliberately not done

- No fallback split for over-window chunks (owner's decision); the count is the evidence.
- The minimal demo untouched (its documents are passage-sized).
- The Rust build, the fixtures, the shipped artefact and the phone untouched; the Rust
  oracle for the demo build is retired with its reason — the 019 slice-parity record stays
  as history.
- `measure --against` stays in the code (valid between two chonky builds); the README no
  longer proposes the Rust slice as the second side.
- No CI job (standing rule: nothing model-backed in CI).
