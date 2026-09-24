## 027 (PR C) — the surfaces, and the measurement

The last of three pull requests for optional sparse lexical expansion. It carries the option to
every surface, the three-dataset measurement that decides it, and the report.

### The measurement (moved here from PR B by the owner's decision)

**Full pipeline**, `hybrid-sparse-rerank-v1` against `hybrid-rerank-v3`, where SC-001 and
SC-002 are judged:

| | nDCG@10 | Δ | Recall@100 | Δ |
|---|---|---|---|---|
| SciFact | 0.72166 | −0.0003 | 0.95500 | 0.0000 |
| NFCorpus | 0.35767 | −0.0048 | 0.32166 | +0.0007 |
| FiQA | 0.40680 | **+0.0172** | 0.71325 | +0.0073 |

- **SC-001 passes:** FiQA +0.0172 against the required +0.010.
- **SC-002 passes:** nearest NFCorpus −0.0048 against −0.005.
- **SC-003:** `hybrid-rerank-v3` reproduces Feature 026's record in every per-query score.
  `hybrid-baseline-v2`'s committed record predates the eight-bit embedder, so its re-run is the
  comparator below, not a reproduction.
- **SC-005 passes:** the lexical stage grows 2.49× / 3.86× / 3.42× against 4×.
- **Cross-check (T026):** the harness gives a byte-identical SciFact run to the spike's own
  tools on the same weights.

**Fused only** (`hybrid-sparse-v1` against `hybrid-baseline-v2`): SciFact −0.0070, NFCorpus
−0.0056, FiQA +0.0308. The owner accepted this as the option's documented cost:
[ADR-0017](../../docs/adr/0017-sparse-option-without-reranking.md), option A. Pair the option
with the re-ranker and use it for FiQA-shaped corpora; every surface below says so.

Records: `specs/027-sparse-lexical-expansion/runs/`. That holds the twelve reports, `sizes.json`
(sizes, encode rate, truncation) and `measure-027.log`, the run's milestone lines that back it.

### The surfaces

- **FFI:** `IndexConfig.sparse` takes a `SparseOptionConfig` (the encoder directory, plus an
  optional scale and boost that default to the engine's). `create` loads the encoder with the
  other models, before the directory is touched, and builds a sparse index with it attached, so
  the handle can `add` at once. `IndexInfo.sparse` and the index's own `format_version` report
  it. `IndexHandle.open` needs nothing new. The config conversion is now a private function that
  hands the sparse part back, so no caller can drop it by accident.
- **Python:** exports `SparseOptionConfig` and `SparseInfo`; the README documents the option and
  its advice.
- **Swift, Kotlin:** the wrappers alias the generated types, so the regenerated packages carry
  the change with no hand-written code.
- **Command line:** `xtriever wiki build --sparse-encoder DIR [--sparse-scale N]
  [--sparse-boost B]` builds a sparse artefact (clap requires the encoder for the other two).
  Its corpus identity names the expansion, so a sparse artefact never shares an identity with a
  plain one, and its build record gains the encoder, truncation count and encoding rate. A
  200-article slice built through it: 873 passages at 5.2 per second, descriptor version 3,
  verify PASS.
- **Packagers:** never stage the encoder; `tests/packagers.rs` guards that.
- **Pipeline:** `HybridIndex::sparse_encoder()` lends the attached encoder to a caller that
  encodes passages itself (the command line's cached build).

### Tests

The red checkpoint came first (Rule 4) and went through two review rounds:
- FFI `tests/sparse.rs`: the wire-built index, reopened without the encoder, answers exactly as
  the pipeline's own `create_sparse` index.
- Python `test_sparse.py`: strict searches; the index must differ from a plain build.
- The command line's `sparse_option` tests and the corpus identity test.
- The packager guard.

The FFI tests' wire helpers moved into `tests/support`.

**Local gate:**

- fmt, clippy (`-D warnings`), `cargo deny`: pass.
- iOS, iOS simulator and Android checks: pass. wasm32 fails on `getrandom`/`errno`, as tracked.
- `cargo nextest run --workspace`: 410 passed.
- FFI with models, single-threaded: 26 of 26. Pipeline sparse suite: 14 of 14. Encoder oracle
  and load paths: 9 of 9.
- Command line: 20 of 20.
- `gen_026` and `gen_027` checks: pass. `check-no-stubs.sh`: pass.
- Python: 37 of 37.
- iOS package (models and fixtures) builds. Swift tests on the iPhone 18 Pro simulator
  (iOS 27.0, Release, arm64): **17 passed, 1 skipped** after the fix below; the first run had
  16 passed, 1 skipped, 1 failed.
  - The skip is `WikipediaTests`, which needs the Wikipedia artefact staged
    (`--with-wiki`), not staged for this run.
  - The failure predates this feature: `ErrorTests.testTheWrongModelAsEmbedderIsAModelError`
    expects "bytes" or "sha256" in the error. Since Feature 026 PR B (#31) the embedder refuses a
    directory holding neither of its weights files earlier, with "holds neither
    model.safetensors nor all-MiniLM-L6-v2.Q8_0.gguf". No 027 commit touches the loader or the
    test, and the Swift tests were not run in Feature 026 (no device).
  - **Fixed here, by the owner's decision:** the assertion now requires the refusal Feature 026
    introduced ("holds neither"). That is as strict as before, not looser: the test still
    requires a `Model` error, and now names the exact reason. It is the only change to an
    existing test's expectation in this pull request.
- Android package builds, and the Kotlin library tests pass on the emulator.

Report: [`specs/027-sparse-lexical-expansion/report.md`](report.md).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
