# Report: Chonky as an Optional Chunker for the Wikipedia Demo Build

**Status**: done — `wikidemo build --chunker {contract,chonky}`, default `contract`; the
default build is the shipped recipe again (parity with the Rust slice **PASS**); chonky is
an install-time extra.

## Verdict

| Criterion | Result |
|---|---|
| SC-001 the default slice vs the Rust slice | **PASS** — identity `20949fb4…` equal, counts equal, 8,529 documents equal, 20 queries × 4 depths: lexical bit-identical 20/20, fused order identical 20/20, dense and re-rank max |Δ| 0, **800/800 hits identical on every score bit**, order checked at every depth (`runs/parity-…`) |
| SC-002 the fixture replay and the chonky tests | 48 + 9 cases byte-identical (`test_contract.py`); the 021 tests pass behind the option (`test_chonky.py`, 9 tests incl. the real splitter on the snapshot's first article) |
| SC-003 the refusal without the extra | in-process test: refused in < 1 s with the install command; for real, in a venv without the extra: `build --chunker chonky` → exit 1 in 0.9 s wall with the install command, nothing written; `build --limit 3` (default) in the same venv → exit 0, `chunker: 008 contract`, 30 passages. The poisoned-import test proves a default build touches none of `chonky`, `transformers`, `torch` |
| SC-004 scope | `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/ specs/*/baselines` empty; `apps/` diff 884 + / 255 − over 15 files (below), spec documents 842 lines, three run records 862 lines |

## Red checkpoint (C1)

`pytest apps/python-wiki-demo/tests -q --continue-on-collection-errors`: **2 failed, 53
passed, 5 errors** — the five collection errors were the new names (`wikidemo.contract`,
`wikidemo.chonky_chunker`, `chunking.make_chunker`, `record.CONTRACT_CHUNKER` /
`CHONKY_CHUNKER`), the two failures `test_cli`'s `--chunker` option and `needs_for`.
Green: **90 passed** (65 model-free); the minimal demo's 6 unchanged.

## The default slice (2,000 articles, `MacBookPro18,3`, 10 threads)

| | Rust slice (019) | Python slice, 019 | **Python slice, 023** |
|---|---|---|---|
| passages | 8,529 | 8,529 | **8,529** |
| over window | 0 | 0 | **0** (median 214 positions, p90 252, max 256) |
| identity | `20949fb4…` | `20949fb4…` | **`20949fb4…`** |
| build | — | 14.6 min | **14.3 min** (chunk 6.1 s, embed 851 s) |
| parity vs the Rust slice | — | PASS | **PASS** |

The tokenizer load (`load_splitter`) is 20 ms for the contract chunker; the chonky model's
was 3.2 s.

## The option (200 articles, `--chunker chonky`)

197 selected, **950 passages, 87 over the window (9.2 %** — 021's share on 2,000 articles
was 9.2 % too), median 80 positions, p90 238, max 4,745; split 16 s, embed 96 s, total
117 s; the chonky block in the sidecar and in `about`; identity `82ea207c…`.
`measure --against target/xt-wiki-slice-rs` on it: FAIL on the identity line, as designed.
The refusals: a missing model directory → the fetch command in 0.07 s; `--chunker whole` →
argparse's usage error, exit 2.

## What changed (`apps/python-wiki-demo/`)

- `pyproject.toml`: `chonky`, `transformers`, `torch` moved to the optional extra `chonky`
  (the 021 pins); `tokenizers` stays; a `chonky` test marker.
- `wikidemo/chunking.py`: the shared module — `WINDOW`, `BuildError`, `Window`,
  `passage_text`, `CHUNKERS`, `DEFAULT_CHUNKER`, `make_chunker` (branch-local imports).
- `wikidemo/contract.py` (new): the 019 chunker copy restored from commit `802cf72` —
  the contract steps and constants verbatim (`diff <(git show 802cf72:…/chunking.py)
  …/contract.py` differs by 96 lines: the header, the moved shared members and the
  `ContractChunker` class around 019's `Pricer` / `documents_for`).
- `wikidemo/chonky_chunker.py` (new): 021's `Splitter` and `documents_for` as
  `ChonkyChunker`; `ensure_extra` (`importlib.util.find_spec`, no import).
- `wikidemo/build.py`, `cli.py`, `record.py`: `--chunker`, `needs_for` per chunker, the
  early refusal before the snapshot is hashed, the two blocks.
- tests: `test_contract.py` (restored 019), `test_chonky.py` (021's), `test_chunking.py`
  (the chooser), `test_build.py` / `test_cli.py` / `test_record.py` / `conftest.py` updated.
- `README.md`: the default recipe with the parity claim back; the option.

## Deliberately not done

- **No whole-article chunker.** 022's rule recommended nothing; for Wikipedia the shipped
  recipe (the contract) is the sensible default and chonky the option.
- **No fallback** from chonky to the contract when the extra or the model is missing — the
  index's identity would change silently.
- **The full 2,000-article chonky slice not rebuilt** — the code path is 021's and its
  record stands; the 200-article build proves the option end to end.
- **No Rust change**: the engine never chunks; the Rust CLI keeps its 008 chunker.
- **The minimal demo untouched**; no CI job (nothing here runs without models).
