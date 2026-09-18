# Research: Chonky as an Optional Chunker for the Wikipedia Demo Build

Read on 2026-09-18: the demo at `main` (`build.py`, `chunking.py`, `cli.py`, `inputs.py`,
`record.py`, `about.py`, `measure.py`, the tests, `pyproject.toml`, `README.md`); the
Feature 019 files at commit `802cf72` (`chunking.py` 272 lines, `test_chunking.py`,
`build.py`, `pyproject.toml`); the 021 plan/research; `target/xt-wiki-slice-rs/index/corpus.json`;
the Python 3.12 `importlib.util.find_spec` behaviour (verified in the demo's venv).

## D1 — What is on disk, and what parity means here

`target/xt-wiki-slice-rs` (the Rust CLI's 2,000-article slice from Feature 019) is present:
2,000 read, 1,970 selected, **8,529 passages**, `passages_over_window` 0, chunker block
`{"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}`,
identity `20949fb44303f59039ae21d3aef74ca20ea8f050277676ed5d7e2623dd966133`. The 019
Python slice (`target/xt-wiki-slice-py`) carries the same identity — the parity 019
established. This feature rebuilds the Python slice with the restored code and runs
`measure --against target/xt-wiki-slice-rs`: identity equal, counts equal, documents equal,
order identical at every depth, lexical bits exact — the 019 rule, unchanged in
`measure.py`. The engine has not changed since 019 (020–022 touched no crate), so the Rust
slice needs no rebuild; if it were missing, 019's quickstart Step 5 rebuilds it (~20 min).

## D2 — The chunker interface (`chunking.py`)

Shared, kept in `chunking.py`: `WINDOW = 256`, `BuildError`, `Window` (the embedder's
tokenizer, `token_count`), `passage_text`. New there:

- `CHUNKERS = ("contract", "chonky")`, `DEFAULT_CHUNKER = "contract"`;
- `make_chunker(name, paths, window) -> ContractChunker | ChonkyChunker`: a dict lookup on
  the name; imports the chonky module only for `"chonky"` (so a default build never touches
  it — SC-003).

Each chunker object has three members and nothing else the build reads:

- `documents_for(article) -> tuple[list[xtriever.Document], list[int]]` — the passages and
  each passage's position count (`Window.token_count(passage_text)`), the 021 shape, so
  `build.py`'s counting and `chunking` statistics work for both;
- `block: dict` — the sidecar's chunker block (D9 of 019 / D5 of 021);
- `label: str` — the completion line (`008 contract (256 - token_count(title))` /
  `chonky (mirth/…, revision 01d8aae…)`), the same words `about.chunker_label` prints.

No base class: two classes with the same attribute names (Rule 7).

## D3 — The contract chunker, restored (`contract.py`)

`git show 802cf72:apps/python-wiki-demo/wikidemo/chunking.py` minus what moved to
`chunking.py` (`WINDOW`, `BuildError`, `passage_text`) and minus `Pricer.token_count`'s
role as the window counter (the `Window` class does that now; `Pricer` keeps `cost`, memoised,
and its own `token_count` for the title budget — both read the same `tokenizer.json`, one
`Tokenizer` instance shared by passing `window` in). The 008 steps (`is_ws`, `trim`,
`collapse`, `paragraphs`, `sentences`, `words`, `byte_len`, `fragments`, `chunk`) are
byte-for-byte the 019 text. `ContractChunker(window)`: `documents_for` is 019's
`documents_for(article, pricer)` plus the position count per passage; `block =
record.CONTRACT_CHUNKER`; `label = "008 contract (256 - token_count(title))"`.

The over-window count for a contract build is 0 by construction (the budget is the window
minus the title) — asserted in the build test, not assumed.

Tests restored from `802cf72:…/tests/test_chunking.py` into `test_contract.py`: set A (48
cases, cost = words), set B (nine articles, `unit_costs`), the title-fills-window error, the
shaping test, the budget-split test — with the `documents_for` calls adapted to the class.

## D4 — The chonky chunker, moved (`chonky_chunker.py`)

021's `Splitter` (lazy `from chonky import ParagraphSplitter`, `chunks` with the partition
check) and its `documents_for` become `ChonkyChunker(model_dir, window)` with `block =
record.CHONKY_CHUNKER`, `label = "chonky (mirth/chonky_distilbert_base_uncased_1, revision
01d8aae…)"`. The module is named `chonky_chunker`, not `chonky`, so `from chonky import …`
inside the package can never resolve to itself. 021's tests move to `test_chonky.py`
unchanged in substance (the stub splitter, the partition enforcement, the offsets, the
over-window count, the real splitter on the snapshot's first article).

## D5 — Refusing a chonky build without the extra

`chonky_chunker.ensure_extra()`: `importlib.util.find_spec("chonky") is None` → `BuildError`
with the install command `pip install -e 'apps/python-wiki-demo[chonky]'` (or the `uv pip`
form the README uses). `find_spec` does not import (no torch load) and returns `None` when
`sys.modules["chonky"]` is `None` — verified in the venv — which is how the test simulates
a missing extra in-process (`monkeypatch.setitem(sys.modules, "chonky", None)`).

Order in a chonky build: `main` → `resolve` → `require(needs_for)` (the paths: embedder,
re-ranker, snapshot, manifest, chonky model — cheap `exists` checks, before anything loads,
as 019/021) → `run_build` → `ensure_extra()` → `build()` (which starts with
`verify_snapshot`, the 400 MB hash). So a missing model is reported by the input check and
a missing extra by `ensure_extra`, both before the snapshot is opened (spec FR-005, FR-006,
edge cases). No fallback to the contract chunker on either — the user asked for chonky.

## D6 — The extra and the test marker

`pyproject.toml`: `[project.optional-dependencies] chonky = ["chonky==0.1.7",
"transformers==5.17.0", "torch==2.14.0"]` — the 021 pins moved, not re-resolved;
`dependencies` keeps `xtriever>=0.1.0` and `tokenizers==0.23.2`. Install for the full suite:
`uv pip install --python .venv/bin/python ../../target/wheels/xtriever-*.whl -e ".[test,chonky]"`.

Tests: the `models` marker keeps meaning the two engine models and the 007 fixture index;
the chonky model leaves `missing_for_models`. A new `chonky` marker skips when
`find_spec("chonky") is None` (reason: the install command) or the model directory lacks
`model.safetensors` (reason: the fetch command). The tests that need it: the real-splitter
partition test and the chonky build test. `-m "not models"` stays the model-free subset.

## D7 — The option on the command line

`build --chunker {contract,chonky}` (argparse `choices`, `default="contract"`, help naming
the default and what each is); `--chonky DIR` stays as the model directory. `needs_for`:
`["embedder", "reranker", "snapshot", "manifest"]` plus `"chonky"` when
`args.chunker == "chonky"`. The 021 test "a missing splitter model is refused before
loading torch" becomes a `--chunker chonky` test; a new one asserts a default build lists
four inputs.

## D8 — What is measured and recorded

1. `wikidemo build --limit 2000 --out target/xt-wiki-slice-py-023` (default): the build
   record → `runs/slice-contract-<machine>-<stamp>.json`; expected 8,529 passages, 0 over
   window, identity `20949fb4…`, ~17 min.
2. `wikidemo measure --artefact target/xt-wiki-slice-py-023 --against target/xt-wiki-slice-rs
   --out specs/023-optional-chonky-chunker/runs/parity-<machine>-<stamp>.json`: verdict PASS.
3. `wikidemo build --chunker chonky --limit 200 --out target/xt-wiki-slice-chonky-200`: the
   record → `runs/slice-chonky-200-<machine>-<stamp>.json` (the chonky block, over-window
   count > 0; ~2 min). The 021 full chonky slice record stands.
4. The refusal without the extra and the poisoned-import default build are tests, not
   records; the refusal time is asserted (< 1 s).

## D9 — README

The recipe's step 3 becomes two paragraphs: the default (the 008 contract chunker —
paragraphs, sentences, words, fragments priced by the embedder's tokenizer, the same
passages as the Rust build, the parity check `measure --against`), and the option
(`--chunker chonky`: what it is, `.[chonky]` and the model fetch, the over-window trade-off
with 021's numbers — 9.2 %, median 82, p90 239 — and that such an index is not the shipped
one). The inputs table marks the chonky model "for `build --chunker chonky`"; the install
section names the extra; the run-order block builds with the default and shows the chonky
lines as optional; the "Not the shipped index" paragraph becomes "The same index as the Rust
build" with the chonky caveat. `build --help` and `build.py`'s docstring updated.

## Alternatives considered

- **A base class / registry for chunkers**: rejected (Rule 7) — two classes and a dict.
- **`try: import chonky` at the start of a chonky build**: rejected — imports torch before
  the refusal decision; `find_spec` decides without importing.
- **Falling back to the contract chunker when chonky is missing**: rejected — a silent change
  of the index's identity; the user asked for chonky, so the build stops.
- **Keeping one `chunking.py` with both chunkers**: rejected — the restoration would not be
  diffable against `802cf72`, and the module would be ~350 lines of two unrelated recipes.
- **Rebuilding the full 2,000-article chonky slice**: not needed — the chonky code path is
  021's, its 2,000-article record stands; a 200-article build proves the option end to end.
