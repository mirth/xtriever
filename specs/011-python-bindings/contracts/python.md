# Contract: the `xtriever` Python package

**Feature**: `011-python-bindings` | **Date**: 2026-09-15

## 1. Surface

`import xtriever` exposes exactly the generated names (research D1, D4) plus `__version__`:

```python
xtriever.IndexHandle.open(index_dir: str, embedder_dir: str, reranker_dir: str | None, load_path: LoadPath) -> IndexHandle
xtriever.IndexHandle.create(index_dir: str, config: IndexConfig, embedder_dir: str, reranker_dir: str | None, load_path: LoadPath) -> IndexHandle
handle.info() -> IndexInfo
handle.search(query: str, options: SearchOptions) -> SearchResponse
handle.add(docs: list[Document]) -> None
handle.add_embedded(docs: list[Document], vectors: list[list[float]]) -> None
handle.delete(external_ids: list[str]) -> None
handle.commit() -> None
handle.merge() -> None
handle.contains(external_id: str) -> bool
```

Records are keyword-constructed dataclass-like objects (`SearchOptions(k=10)`,
`Document(external_id="d1", fields={"text": FieldValue.TEXT("…")}, chunk=None)`); enums with
payloads are constructed by variant (`FieldKind.TEXT(analyzer="standard_en")`,
`FieldValue.KEYWORD("a")`); plain enums are `enum.Enum` (`LoadPath.MMAP`). Every method
releases the interpreter lock for the duration of the engine call.

## 2. Errors

`xtriever.XtrieverError` is an `Exception`; every engine error kind is a nested subclass:
`Schema`, `InvalidQuery`, `UnknownField`, `DimensionMismatch`, `NotFound`, `Model`, `Corrupt`,
`FingerprintMismatch`, `BudgetExhausted`, `Io`, `Backend`. `str(e)` carries the engine's
message. Which one is raised is the engine's decision, unchanged from Rust and Swift.

## 3. Identity

For the same directory, models, query and options, `handle.search` returns the Rust
`HybridIndex::search`'s result field for field, score bits included — the 007 goldens are the
oracle (SC-002). An index created and filled from Python over the 005 fixture's documents
yields the same hits as the Rust-built fixture index for every fixture query (SC-007).

## 4. Packaging

- Build: `maturin build --release -m python/pyproject.toml` from a checkout with the pinned
  Rust toolchain → one wheel `xtriever-<version>-py3-none-<platform>.whl` under `target/wheels/`.
- Install: `pip install <wheel>` (or `uv pip install`) into Python ≥ 3.9 on macOS arm64 or
  Linux x86_64; no Rust toolchain needed to install or import.
- Contents: `xtriever/__init__.py`, `xtriever/_ffi/xtriever_ffi.py`, `xtriever/_ffi/__init__.py`,
  the shared library. No models, no fixtures, no generated code in git.
- Version: the workspace crate version (`dynamic = ["version"]`).
- Models: `scripts/fetch-model.sh` and `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json`;
  the package never downloads.
