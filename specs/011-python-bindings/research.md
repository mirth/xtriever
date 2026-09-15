# Research: Python Interface and Bindings

**Feature**: `011-python-bindings` | **Date**: 2026-09-15 | **Plan**: [plan.md](./plan.md)

Everything below was tried on this host (macOS arm64, Homebrew CPython 3.12.12 arm64, `uv`
0.9.26, maturin 1.15.0) against the FFI crate as merged at `e7e68e7`.

## D1 — The Python module comes from uniffi's own generator, not a second layer

`xtriever-ffi` is uniffi 0.32.1 with proc-macro exports (`#[uniffi::export]`,
`#[derive(uniffi::Record / Enum / Object / Error)]`). uniffi's generic bindgen —
`uniffi::uniffi_bindgen_main()` (uniffi-0.32.1 `src/cli/mod.rs:8`) — takes
`generate --library <cdylib> --language python --out-dir <dir>` and writes one module,
`xtriever_ffi.py` (2,171 lines), from the same metadata the Swift bindings are generated from.
Tried: the module loads the cdylib by name next to itself (`_uniffi_load_indirect`, generated
`:493–516`: `ctypes.cdll.LoadLibrary(os.path.join(os.path.dirname(__file__), "libxtriever_ffi.dylib"))`
— `.so` on Linux), and against the 007 fixture index with both models:

- **16 / 16 golden pairs bit-identical** (8 queries × with / without the re-ranker): external
  ids and order, fused score (f64 bits), re-rank score (f32 bits), bm25 and dense score bits,
  re-rank rank — read from `swift/Xtriever/Tests/Fixtures/expected.json` and compared as hex.
- **Errors**: `XtrieverError` is a Python `Exception` subclass with one nested class per
  variant — `XtrieverError.Io`, `.Corrupt`, `.Model`, `.BudgetExhausted`, `.Schema`,
  `.InvalidQuery`, `.UnknownField`, `.DimensionMismatch`, `.NotFound`,
  `.FingerprintMismatch`, `.Backend` (generated `:1688–1864`); a missing directory raised
  `Corrupt("…/xtriever-pipeline.json is missing: not a hybrid index")`, a 1 ms strict budget
  raised `BudgetExhausted`. FR-003 is met by the generator with nothing to write.
- **GIL**: `ctypes.cdll` (CDLL) releases the GIL around every foreign call; a second thread
  finished 20 × `sum(range(100_000))` in 24 ms while a 431 ms search was in flight (SC-005).
- **Defaults**: `SearchOptions` already carries `#[uniffi(default = …)]` on every field but
  `k` (`ffi/types.rs:9–30`), so `SearchOptions(k=10)` is enough in Python.

**Decision**: generate with uniffi; the Python package is `__init__.py` re-exports over the
generated module and nothing else (FR-014). **Rejected**: PyO3 (a second surface to keep in
step with the Swift one; links `libpython`, needs a per-interpreter ABI or the limited API);
cffi (would need a hand-written C header, which uniffi would also generate — no gain).

## D2 — Packaging: maturin's `uniffi` mode, driven by the crate's own `uniffi-bindgen` bin

maturin 1.15.0 (`src/binding_generator/uniffi_binding.rs`, read at tag `v1.15.0`): with
`bindings = "uniffi"` it looks for a **bin target named `uniffi-bindgen`** in the crate
(`:99–102`) or the workspace (`:103–106`) and runs
`cargo run --bin uniffi-bindgen [--features <the bin's required-features>] -- generate
--no-format --language python --out-dir <target/maturin/uniffi/<module>> [--config uniffi.toml]
--library <cdylib>` (`:107–123`, `:144–163`); only without such a bin does it fall back to a
`uniffi-bindgen` executable on `PATH` (`:124–127`). The cdylib itself is built separately,
**without** the bin's features — so `--features cli` reaches the bindgen only and never the
shipped library. Tried, with the crate's existing bin switched to dispatch on the first
argument (`generate` → `uniffi_bindgen_main()`, else the Swift entry point the iOS script
uses): maturin ran `target/debug/uniffi-bindgen generate --no-format --language python …
--library target/maturin/libxtriever_ffi.dylib` and produced
`xtriever-0.1.0-py3-none-macosx_11_0_arm64.whl` (2.8 MB; the 6.3 MB release cdylib inside).

Layout that worked: `pyproject.toml` with `manifest-path` pointing at the crate,
`python-source` for the hand-written package and `module-name = "xtriever._ffi"` → the wheel
holds `xtriever/__init__.py` (ours), `xtriever/_ffi/__init__.py` (generated:
`from .xtriever_ffi import *`), `xtriever/_ffi/xtriever_ffi.py`, `xtriever/_ffi/libxtriever_ffi.dylib`.
Installed into a fresh venv with `uv pip install <wheel>`, `import xtriever` opened the fixture
and searched. The wheel tag is `py3-none-<platform>` — ctypes, no CPython ABI — one wheel per
platform serves every supported interpreter.

**Decision**: `python/pyproject.toml` (maturin backend, `dynamic = ["version"]` from the
crate — `0.1.0` today), `python/src/xtriever/__init__.py`, `python/tests/`, `python/README.md`;
the bindgen bin gains the two-line dispatch (the Swift script's flags are untouched: it never
passes `generate`). **Rejected**: the PyPI `uniffi-bindgen` package (must match 0.32.1 exactly
and is one more pin); a hand-rolled `setup.py` copying the cdylib (maturin already audits,
tags and repairs the wheel).

## D3 — Supported interpreters and platforms

The generated module uses `from __future__ import annotations`, `dataclasses`, `typing`,
`enum`, `ctypes` — nothing newer than 3.8. `requires-python = ">=3.9"` (3.8 is end-of-life);
tested on **3.12 and 3.13** locally (both installed, arm64) and 3.12 in CI. Platforms:
macOS arm64 (this host) and Linux x86_64 (CI, ubuntu-latest, `manylinux` tag decided by
maturin's audit — recorded in the report). The system `python3` at `/usr/local/bin` on this
host is an **x86_64** Homebrew build under Rosetta: it cannot load the arm64 cdylib
(`incompatible architecture`) — the quickstart names the arm64 interpreters explicitly, and
the package's import error in that case is ctypes' own `OSError`, stated in the README.

## D4 — The builder on the wire (owner decision Q1 = B)

New wire types in `ffi/types.rs`, mapped one-to-one onto `xtriever_core` / `xtriever_pipeline`
types (which do not change):

| wire | maps to |
|---|---|
| `FieldKind` enum: `Text { analyzer: String }`, `Keyword`, `U64`, `I64`, `F64`, `Bool`, `DateMillis` | `xtriever_core::FieldKind` (`types.rs:67–82`; `AnalyzerId(String)` `:44`) |
| `FieldDef { name, kind, indexed = true, stored = false, boost = 1.0 }` | `xtriever_core::FieldDef` (`:86–98`) |
| `IndexConfig { fields, dense_fields, candidate_depth = 100, rrf_k = 60, rerank_depth = 20 }` | `HybridConfig` (`pipeline/types.rs:25–37`; defaults from `HybridConfig::new`, `:40–48`) |
| `FieldValue` enum: `Text(String)`, `Keyword(String)`, `U64`, `I64`, `F64`, `Bool`, `DateMillis(i64)` | `xtriever_core::Value` (`:48–62`) |
| `Document { external_id, fields: HashMap<String, FieldValue>, chunk: Option<ChunkInfo> }` | `SourceDocument` (`ChunkInfo` is the existing wire record) |

New exports on `IndexHandle` (all through the existing `Mutex<HybridIndex>`, which already
gives `&mut` access):

| export | pipeline call |
|---|---|
| `#[uniffi::constructor] create(index_dir, config, embedder_dir, reranker_dir, load_path)` | `HybridIndex::create(dir, config, embedder)` + the models as `open` loads them |
| `add(docs: Vec<Document>)` | `HybridIndex::add` (`index.rs:369`) — the embedder embeds |
| `add_embedded(docs: Vec<Document>, vectors: Vec<Vec<f32>>)` | `HybridIndex::add_embedded` (`:392`); lengths must match → `Schema` error |
| `delete(external_ids: Vec<String>)` | `HybridIndex::delete` (`:408`) |
| `commit()` | `HybridIndex::commit` |
| `merge()` | `HybridIndex::merge` (`:434`; 008 D10 — one segment for shipping) |
| `contains(external_id) -> bool` | `HybridIndex::contains` (`:323`) |

`open` already opens writable when the lock can be taken (007/008), so a Python program can
also open an existing index and add to it; a read-only open returns the engine's `Io`
"read-only index" on any write. Refusals are the engine's (`Schema` for a dense field not in
the schema or a non-text dense field or an unknown analyzer, `Corrupt` for a non-empty
directory, `DimensionMismatch` for a wrong vector length) — nothing is re-validated in the
FFI. uniffi supports `HashMap<String, Enum>` records and enums with named/unnamed fields
(0.32 docs, "Records", "Enumerations"); the generated Python spells variants
`FieldValue.TEXT("…")`, `FieldKind.KEYWORD()` (as `DegradeReason.STAGE_ERROR` today).

The Swift package sees the new exports at its next generation and ignores them; its goldens
and suite (18 / 18) do not change. **Rejected**: a separate `IndexWriter` object (two handles
over one directory would need two locks and a story for searching while writing; one handle
with the committed/pending views the pipeline already has is the boring choice).

## D5 — Tests and oracles

- **Rust, model-backed (`#[ignore]`, release, `-j 1` as 008 F-007)**: `tests/build.rs` in the
  FFI — create the 005 fixture's schema and documents through the wire types with the real
  embedder, commit, search every fixture query, compare to `fixture_index`'s goldens
  (SC-007 at the boundary); a replace and a delete before/after commit; every refusal above.
- **Python, model-backed** (`pytest -m models`, skipped without the models): `test_search.py`
  — the 16 golden pairs, score bits, explanations, stage reports; `test_options.py` — strict
  vs non-strict at a 1 ms budget, depth 0, explain off, `k = 0`; `test_build.py` — the fixture
  documents from `reference/fixtures/005/hybrid.json` built from Python, reopened, searched:
  equal to the goldens (SC-007); `test_threads.py` — SC-005; `test_overhead.py` — SC-004 as
  `(wall − response.elapsed_ms) / elapsed_ms ≤ 5 %` over the 8 queries, median.
- **Python, model-free** (CI): `test_surface.py` — import, `__version__` equals the crate's,
  the exported names, the exception hierarchy (each nested class is a subclass of
  `XtrieverError`), `SearchOptions` defaults; `test_errors.py` — a bad embedder directory
  raises `XtrieverError.Model` (the first thing `open`/`create` does is load the embedder, so
  it is the one refusal reachable without a model; the rest need one and are marked).
- The goldens file is the 007 one; no new goldens (FR-011).

## D6 — CI: one Linux job, model-free

`python` job on `ubuntu-latest`, path-filtered (`crates/xtriever-ffi/**`, `python/**`,
`Cargo.lock`, the workflow): `rustup toolchain install`, `uv` (astral-sh/setup-uv), `uv venv`
+ `maturin`, `maturin build --release -m python/pyproject.toml`, `uv pip install` the wheel,
`pytest -m "not models"`. Release build of the FFI cdylib and its tree on the runner is the
cost (candle, tantivy, tokenizers — estimated 5–7 min cold; `Swatinem/rust-cache` with its
own key); SC-006 says under 10 minutes — measured on the first run and recorded. No model,
no dataset, no macOS job (standing rule).

## D7 — What is *not* changed

- `xtriever-core`, the stage crates, `xtriever-pipeline`, `deny.toml`: untouched.
- The existing FFI surface (`open`, `info`, `search`, every wire type): unchanged; the Swift
  package, its goldens, the harness and the demo app: unchanged and untouched.
- No async facade (spec assumption); no model download helper (assumption); no PyPI publish.
