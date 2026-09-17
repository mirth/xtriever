# Tasks: Chonky Chunking for the Wikipedia Demo Build

**Input**: Design documents from `/specs/021-chonky-wiki-chunking/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md)
(D1–D10), [data-model.md](./data-model.md), [contracts/build.md](./contracts/build.md),
[quickstart.md](./quickstart.md); on disk: the two engine models, the snapshot, the demo
venv (`PY=apps/python-wiki-demo/.venv/bin`), the chonky model files at revision `01d8aae…`
in the local HF cache (from the experiment) or the network to fetch them.

**Tests**: **Mandatory** (Principle II; spec FR-008): the rewritten `test_chunking.py` and the
updated build / record / inputs / cli tests committed red before the code. One PR; two
checkpoints the **owner commits** (the agent runs no git command that changes state):
**C1** = Phases 1–2 (red), **C2** = Phases 3–6 (green, the slice record, docs, report).

**Organization**: Setup (the manifest, the model, the venv) → Foundational (the red tests) →
US2 (the pinned model as an input — the build needs it first) → US1 (the split) → US3
(the recipe reads as one: README) → Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`reference/models/manifest-chonky.json`; `apps/python-wiki-demo/{pyproject.toml,README.md}`;
`apps/python-wiki-demo/wikidemo/{chunking,build,inputs,about,record,cli}.py`;
`apps/python-wiki-demo/tests/{conftest,test_chunking,test_build,test_record,test_inputs,test_cli}.py`;
`specs/021-chonky-wiki-chunking/{runs/,report.md,pr-description.md}`. All commands from the
repository root with `unset SDKROOT`.

---

## Phase 1: Setup

- [X] T001 Write `reference/models/manifest-chonky.json` (contracts/build.md; research D2): `schema_version 1`, `repository "mirth/chonky_distilbert_base_uncased_1"`, `revision "01d8aae08726368a1b1645de2a7086610f2e86a5"`, `local_dir "chonky_distilbert_base_uncased_1"`, the six `files` (`config.json`, `model.safetensors`, `special_tokens_map.json`, `tokenizer.json`, `tokenizer_config.json`, `vocab.txt`) with `bytes` and `sha256` measured from the files at that revision (`~/.cache/huggingface/hub/models--mirth--chonky_distilbert_base_uncased_1/snapshots/01d8aae…/` — `stat -f %z`, `shasum -a 256`; the cache's LFS blob name is the safetensors' sha256, a cross-check), and the `note`; then `scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json` → `PASS` with six `verified` lines into `reference/models/chonky_distilbert_base_uncased_1` (**⛔** a hash mismatch between the cache and the hub download is stop-and-report); confirm `reference/models/` is gitignored for the new directory (`git status --short reference/models` empty)
- [X] T002 `apps/python-wiki-demo/pyproject.toml`: `dependencies` gains `"chonky==0.1.7"`, `"transformers==5.17.0"`, `"torch==2.14.0"` (research D7 — the resolver's versions on 2026-09-17, cited in a comment) and keeps `"tokenizers==0.23.2"` with its comment rewritten ("the embedder's tokenizer, for the over-window count only"); `cd apps/python-wiki-demo && uv pip install --python .venv/bin/python -e ".[test]"`; `.venv/bin/python -c "import chonky, torch, transformers, tokenizers; print(torch.__version__, transformers.__version__, tokenizers.__version__)"` → `2.14.0 5.17.0 0.23.2`; `mkdir -p specs/021-chonky-wiki-chunking/runs`

---

## Phase 2: Foundational — the red tests

- [X] T003 `apps/python-wiki-demo/tests/conftest.py`: `CHONKY = Path(os.environ.get("XTRIEVER_CHONKY_MODEL_DIR", REPO / "reference/models/chonky_distilbert_base_uncased_1"))`; `missing_for_models()` also checks `CHONKY / "model.safetensors"`; export `CHONKY`
- [X] T004 [P] Rewrite `apps/python-wiki-demo/tests/test_chunking.py` (research D8; spec FR-003, FR-004): model-free — `test_chunks_are_character_ranges_from_the_splitter`: a `Splitter.__new__`-built instance whose `_splitter` is a stub yielding `["Café au lait.\n\n", "  ", "Deuxième paragraphe — fin."]` for the text that is their concatenation → `chunks(text) == [(0, 15), (15, 17), (17, 43)]`; `test_partition_is_enforced`: a stub yielding slices that do not reach the end → `BuildError` mentioning "partition"; `test_documents_for_shapes_and_offsets`: with that stub and a stub window (`token_count = words + 2`), `documents_for({"id": "9", "title": "T", "text": text}, splitter, window)` → two documents (`"9#0"`, `"9#1"` — the whitespace-only chunk emits none, ordinals contiguous), `fields["text"] == "T\n\nCafé au lait."` (stripped) and `"T\n\nDeuxième paragraphe — fin."`, `chunk.byte_start / byte_end` such that `text.encode()[bs:be].decode() == "Café au lait.\n\n"` (the unstripped chunk; multi-byte `é` and `—` covered), `parent == "9"`, and the returned over-window count `0`; `test_over_window_is_counted`: a stub window returning 300 for one passage → count 1 and the document still emitted; `test_passage_text`; `models` — `test_real_splitter_partitions_articles`: `Splitter(CHONKY)` on the three synthetic build articles and on one ~20,000-character prose text built from repeated paragraphs (`"\n\n".join(…)`) → for each, `"".join(text[s:e] for s, e in chunks(text)) == text`, ranges contiguous from 0 to `len(text)`, at least two chunks for the long text
- [X] T005 [P] Update `apps/python-wiki-demo/tests/test_build.py`: `assert sidecar["chunker"] == CHUNKER` stays but `CHUNKER` is now the chonky block (assert its three keys and the revision literal `01d8aae08726368a1b1645de2a7086610f2e86a5` explicitly too); `counts["passages_over_window"]` is an int ≥ 0; `record["chunking"]` has keys `{passages, over_window, token_median, token_p90, token_max}` with `passages == counts["passages"]`; the completion stdout contains `chunker: chonky (` and `passages over the embedder window: `; the `about` assertion adds `chunker: chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae` and `passages over the embedder window: `; the two-paragraph article may now yield one or two passages — assert `passages >= 2` overall as before (three articles → the two selected give ≥ 2)
- [X] T006 [P] Update `apps/python-wiki-demo/tests/test_record.py`: `test_identity_of_the_shipped_corpus` uses a local literal `SHIPPED_CHUNKER = {"version": 1, "budget": "256 - token_count(title)", "cost": "MiniLmEmbedder::token_count(unit) - 2"}` (the function is unchanged; the shipped identity still reproduces) and a new `test_chunker_block_is_chonky`: `CHUNKER == {"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}` and `corpus_identity(SNAPSHOT, EXCLUSIONS, CHUNKER, FINGERPRINT) != SHIPPED_IDENTITY`
- [X] T007 [P] Update `apps/python-wiki-demo/tests/test_inputs.py` and `test_cli.py`: `p.chonky == REPO / "reference/models/chonky_distilbert_base_uncased_1"` by default, `XTRIEVER_CHONKY_MODEL_DIR` and `--chonky` override; `first_missing(p, ["chonky"]) == ("the chonky splitter model", p.chonky / "model.safetensors", "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json")`; `run_cli(["build", "--out", tmp, "--limit", "1", "--chonky", "/nonexistent"])` with the other inputs present → exit 1 and `wikidemo: missing the chonky splitter model: /nonexistent/model.safetensors` in under a second (no torch import — assert `"torch" not in sys.modules` after the call when it was not imported before)
- [X] T008 Quickstart Step 1: `$PY/pytest apps/python-wiki-demo/tests/test_chunking.py apps/python-wiki-demo/tests/test_build.py apps/python-wiki-demo/tests/test_record.py apps/python-wiki-demo/tests/test_inputs.py apps/python-wiki-demo/tests/test_cli.py -q` → `test_chunking` errors on import (`Splitter`, `Window`), the others fail on the chunker block / the new input; record in `specs/021-chonky-wiki-chunking/report.md` ("Red checkpoint"). **⛔ Checkpoint C1 — the owner commits** the manifest, the pyproject, the tests, the report stub

---

## Phase 3: User Story 2 — The model is pinned and fetched like the others (Priority: P1)

**Goal**: the splitter model is a checked input, loaded from disk.

**Independent Test**: the fetch PASS (T001), the missing-input message (T007), a build with the network off (T014).

- [X] T009 [US2] `apps/python-wiki-demo/wikidemo/inputs.py` (research D6): `ENV["chonky"] = "XTRIEVER_CHONKY_MODEL_DIR"`, `DEFAULTS["chonky"] = "reference/models/chonky_distilbert_base_uncased_1"`, `PRODUCERS["chonky"] = "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json"`, `WHAT["chonky"] = "the chonky splitter model"`, `Paths.chonky`, `resolve` picks it (flag `chonky`), `_sentinel` → `paths.chonky / "model.safetensors"`; `cli.py`: `build` gains `--chonky DIR` (help: the splitter model directory; env; default) and `needs_for("build")` → `["embedder", "reranker", "snapshot", "manifest", "chonky"]`
- [X] T010 [US2] `apps/python-wiki-demo/wikidemo/record.py`: `CHUNKER = {"name": "chonky", "model": "mirth/chonky_distilbert_base_uncased_1", "revision": "01d8aae08726368a1b1645de2a7086610f2e86a5"}` with the comment rewritten (the block names the splitter and its pinned revision; the shipped artefact's block is the 008 contract's); `$PY/pytest apps/python-wiki-demo/tests/test_record.py apps/python-wiki-demo/tests/test_inputs.py apps/python-wiki-demo/tests/test_cli.py -q` → green

---

## Phase 4: User Story 1 — The build splits articles with chonky (Priority: P1)

**Goal**: `chunking.py` rewritten (< 80 lines); `build.py` uses it; the counts, the sidecar, the record, `about`.

**Independent Test**: quickstart Steps 2–3.

- [X] T011 [US1] Rewrite `apps/python-wiki-demo/wikidemo/chunking.py` (research D3; data-model): module docstring (what chonky is — a token-classification model that yields the text as contiguous slices at predicted paragraph breaks — the model pin, the partition check, the over-window trade-off and its numbers, and that the Rust build's contract chunker is no longer what the demo uses); `WINDOW = 256`; `class BuildError(Exception)`; `class Splitter`: `__init__(self, model_dir)` imports `from chonky import ParagraphSplitter` lazily and sets `self._splitter = ParagraphSplitter(model_id=str(model_dir), device="cpu")`; `chunks(self, text) -> list[tuple[int, int]]`: `start = 0; out = []; for piece in self._splitter(text): out.append((start, start + len(piece))); start += len(piece)`; `if start != len(text) or any(text[s:e] != piece …)` → simpler: track `pieces` and assert `"".join(pieces) == text`, else `raise BuildError("the splitter did not return a partition of the text")`; `class Window`: `__init__(self, embedder_dir)` → `tokenizers.Tokenizer.from_file(<dir>/tokenizer.json)`, `no_truncation()`, `no_padding()`; `token_count(s)`; `over(self, s) -> bool` = `token_count(s) > WINDOW`; `passage_text(title, body)`; `documents_for(article, splitter, window) -> tuple[list[xtriever.Document], int]`: walk the ranges with a running `byte_start` (`byte_start += len(text[prev_end:start].encode())` is unnecessary since ranges are contiguous: `byte_end = byte_start + len(chunk.encode())`, next `byte_start = byte_end`), skip `chunk.strip() == ""` (still advancing the byte cursor), `Document(external_id=f"{id}#{ordinal}", fields={"title": TEXT(title), "text": TEXT(passage_text(title, chunk.strip()))}, chunk=ChunkInfo(parent=id, ordinal=ordinal, byte_start, byte_end))`, `over += window.over(passage)`; wrap a splitter `BuildError` with the article id; `wc -l` < 80
- [X] T012 [US1] `apps/python-wiki-demo/wikidemo/build.py`: import `Splitter, Window, BuildError, documents_for`; after the handle is created, `splitter = Splitter(paths.chonky)` (timed — its load goes into a new `phases["load_splitter"]`) and `window = Window(paths.embedder)`; the loop calls `docs, over = documents_for(article, splitter, window)` and adds `over` to `counts["passages_over_window"]`, collecting per-passage token counts? — no: `documents_for` returns the count only; for the record's statistics collect `window.token_count(text)` per passage inside `documents_for` and return them (`tuple[list[Document], list[int]]`; `over = sum(n > WINDOW for n in tokens)`) — one list, one `statistics.median`; the record gains `"chunking": {"passages": …, "over_window": …, "token_median": …, "token_p90": …, "token_max": …}` (p90 = `sorted(tokens)[int(0.9 * len(tokens))]`, or nulls when no passages); `phases_ms` gains `load_splitter`; the completion output adds `passages over the embedder window: N (P %)` and `chunker: chonky (mirth/chonky_distilbert_base_uncased_1, revision 01d8aae…)`; the docstring's recipe line becomes "split each article with the chonky splitter"
- [X] T013 [US1] `apps/python-wiki-demo/wikidemo/about.py`: after the `articles:` line, `passages over the embedder window: {counts['passages_over_window']:,}` and `chunker: ` — `chonky ({model}, revision {revision[:7]}…)` when the block has `name == "chonky"`, else `008 contract ({budget})` when it has `budget`, else `(unknown)`; `$PY/pytest apps/python-wiki-demo/tests -q` → all green (the chonky-backed tests run — the model is on disk; **⛔** a partition failure on the real splitter is stop-and-report); `wc -l apps/python-wiki-demo/wikidemo/chunking.py`
- [X] T014 [US1] Quickstart Step 3: `time $PY/wikidemo build --limit 2000 --out target/xt-wiki-slice-chonky 2>/tmp/021-build.err` (**⛔** over 30 minutes or a partition error is stop-and-report); `$PY/wikidemo about --artefact target/xt-wiki-slice-chonky` shows the chonky chunker line and the over-window count; `$PY/wikidemo search --artefact target/xt-wiki-slice-chonky --snippet 80 -k 3 "April"` returns April's passages; a second build of `--limit 20` into a scratch directory with the network off (`XTRIEVER_…` unchanged; verify by `HF_HUB_OFFLINE=1` and, separately, by watching that no download line appears — the model loads from disk in the first build already); the missing-input run (`XTRIEVER_CHONKY_MODEL_DIR=/nonexistent … --limit 1`) → exit 1 in under a second; copy `target/xt-wiki-slice-chonky/wiki-build.json` to `specs/021-chonky-wiki-chunking/runs/slice-chonky-$(sysctl -n hw.model)-<UTC stamp>.json` (no hostname in it — grep); paste the counts, the `chunking` block, the phases (split, embed, total) into the report beside 019's slice (8,529 passages, 14.6 min)

---

## Phase 5: User Story 3 — The recipe reads as one (Priority: P2)

- [X] T015 [P] [US3] `apps/python-wiki-demo/README.md`: the "Build an index" recipe step 3 becomes "split each article with the chonky splitter (`chunking.py`: `Splitter`) — a small fine-tuned model that returns the article as contiguous slices at predicted paragraph breaks; every non-empty slice is one passage with its byte range; chunks longer than the embedder's 256-token window are embedded from their first 256 word-pieces and indexed whole for lexical search (about 10 % on Wikipedia — median 77 tokens, p90 248; the count is in `about`)"; the inputs table gains the chonky model row (`scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json`, `XTRIEVER_CHONKY_MODEL_DIR`); the install line notes torch; the "same index as the Rust build" paragraph and "The check" section are replaced by two sentences: the demo's passages are chonky's, not the 008 contract's, so a demo-built index is not the shipped one and `measure --against` the Rust slice would (correctly) fail — the oracle for the split is that it partitions the text (tested), and `measure` over the shipped artefact is unchanged; the 013 schema note stays; the run-order block's build comment updated (`~8.5k passages` → whatever the record says)
- [X] T016 [P] [US3] `apps/python-wiki-demo/wikidemo/build.py` and `chunking.py` docstrings re-read as the recipe: verify, exclude, split, add, commit, merge, sidecars — no mention of pricing, budgets or the contract chunker remains (`grep -n "Pricer\|budget\|contract" apps/python-wiki-demo/wikidemo/*.py` → only the `about` label for the shipped artefact's block)

---

## Phase 6: Polish

- [X] T017 Gate (quickstart Step 4): `git diff --stat main -- crates/ swift/ python/src apps/python-minimal-demo reference/fixtures specs/*/baselines` empty; `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo deny check` unchanged; `$PY/pytest apps/python-wiki-demo/tests -q` green (counts into the report); `$PY/pytest apps/python-minimal-demo/tests -q` → 6 passed; `wc -l apps/python-wiki-demo/wikidemo/chunking.py` < 80; `grep -rn "$(hostname -s)\|$USER" specs/021-chonky-wiki-chunking apps/python-wiki-demo --exclude-dir=.venv` → nothing; `git status --short` shows only the intended files (`reference/models/chonky_…` absent — gitignored)
- [X] T018 Write `specs/021-chonky-wiki-chunking/report.md` (verdict; the red checkpoint; the slice's numbers beside 019's — passages, over-window share, split time, build time; the partition check; the model pin; SC-001–SC-004; "Deliberately not done": no fallback split, the minimal demo untouched, the Rust oracle retired and why, no CI job) and `specs/021-chonky-wiki-chunking/pr-description.md` (what changed in four lines, the numbers, the dependencies added, the attribution line). **⛔ Checkpoint C2 — the owner commits**, pushes and opens the PR

---

## Dependencies & Execution Order

T001 → T002 → T003 → (T004 ‖ T005 ‖ T006 ‖ T007) → T008 (C1, owner) → T009 → T010 → T011 →
T012 → T013 → T014 → (T015 ‖ T016) → T017 → T018 (C2, owner).

### User story completion order

US2 (the input and the pin — the build cannot load without it) → US1 → US3.

### Parallel opportunities

The four test updates (T004–T007); T015 ‖ T016; T015 can be drafted while T014's slice
build runs (~15 min).

## Implementation Strategy

**MVP** = Phases 1–4 (the pinned model, the rewritten chunker, the build green on the
synthetic snapshot and the slice). **Rule 6 stop-points**: T001 (a hash mismatch), T008
(red), T013 (a partition failure on the real splitter), T014 (over budget or a partition
error on the slice), T017 (any gate failure). Never the partition check, never the pin.
