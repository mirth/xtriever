# Tasks: Sparse Expansion Spike

**Input**: Design documents from `/specs/012-sparse-spike/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/spike-cli.md](./contracts/spike-cli.md),
[quickstart.md](./quickstart.md); the pinned BEIR sets under `reference/datasets/beir/`; the
004 vector cache `target/xt-dense-cache/<d>/` and the 006 indexes `target/xt-rerank-index/<d>/`
(present); `reference/gen_003_fixtures.py` (the scorer); `uv` at `/opt/homebrew/bin/uv`.

**Tests**: **Mandatory** (Principle II). Phase 2 lands the checks red: the scorer probe and
the SC-001 export check call subcommands that do not exist yet; the recipe check and the BM25
unit check are pytest files importing functions the script does not have. The story phases
turn them green in order.

**Organization**: Setup (env, pins, manifests) → Red checks → US1 (encode, variants, fusion —
the table) → US2 (costs, quantisation, projection) → US3 (the second model, the model table)
→ Polish (summary, report, gate). One PR; commits: **C1** = Phases 1–2 (red), **C2** = Phases
3–5, **C3** = Phase 6.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: US1–US3 from spec.md; setup, foundational and polish tasks carry no label
- Every task names its file path(s). Design facts are research D-numbers, data-model rows and
  the contract — read them before implementing. **Rule 6 stop-points are marked ⛔.**

## Path Conventions

`reference/sparse_spike.py`, `reference/requirements-012.{in,txt}`, `reference/.venv-012/`
(git-ignored by the existing `reference/.venv-*/` rule), `reference/models/manifest-sparse-doc-v{2,3}.json`,
`reference/tests_012/` (pytest checks, run with the spike venv); caches `target/xt-sparse-cache/`,
runs `target/xt-sparse-runs/`; committed results `specs/012-sparse-spike/runs/`. Python invoked as
`reference/.venv-012/bin/python` from the repository root.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: The environment and the pinned models.

- [X] T001 Write `reference/requirements-012.in` (a header comment naming the feature and the reuse of the 003/004 pins) with: `torch==2.14.0`, `transformers==5.17.0`, `tokenizers==0.23.2`, `safetensors==0.8.0`, `numpy==2.5.3`, `pytrec_eval==0.5`, `scipy`, `snowballstemmer`, `huggingface-hub==1.31.0`, `pytest`; create the venv `/opt/homebrew/bin/uv venv --python 3.12 reference/.venv-012`, install `pip-tools` and run `pip-compile reference/requirements-012.in -o reference/requirements-012.txt` (the pinned lock, committed, as 003–008 did); `uv pip install -r reference/requirements-012.txt`; confirm `python -c "import torch; print(torch.backends.mps.is_available(), torch.get_num_threads())"`
- [X] T002 Write the `pin` subcommand skeleton of `reference/sparse_spike.py` (contract): argparse with subcommands `pin | encode | export | score | all | summary` (the others `sys.exit("not implemented")` for now — this is a script under `reference/`, outside `check-no-stubs.sh`'s scan of `crates/`); `pin --repo R --out F` resolves `HfApi().model_info(R).sha`, downloads `config.json`, `tokenizer.json`, `model.safetensors`, `idf.json` at that revision into a temp dir with `hf_hub_download(revision=sha)`, and writes the manifest in the 004 shape (`schema_version 1`, `repository`, `revision`, `local_dir`, `files[] {name, bytes, sha256}`) plus `license` (from the card metadata via `model_info.card_data`), `parameters` (67_000_000 as the card states; or counted from the safetensors header), `activation` (`log1p_relu` for v2, `log1p_log1p_relu` for v3 — chosen by the repo name) — data-model "Model manifest"
- [X] T003 Pin both models and fetch them: `pin --repo opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill --out reference/models/manifest-sparse-doc-v3.json`, the same for `…-doc-v2-distill` → `manifest-sparse-doc-v2.json`; then `scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json` and `…-v2.json` (the script's file loop is generic, `scripts/fetch-model.sh:53–70`; if it rejects a manifest field, adapt the manifest, not the script) → `reference/models/<local_dir>/` verified, git-ignored; record the two revision hashes in `specs/012-sparse-spike/report.md` ("Pins")

---

## Phase 2: Foundational — the checks, red

**Purpose**: The oracle and the recipe checks, committed before the encoder exists. **⛔ Commit
C1 at the end of this phase.**

- [X] T004 [P] Write `reference/tests_012/test_scorer.py`: imports `reference/gen_003_fixtures.py` as a module (sys.path) and calls `probe()` (must pass now); loads each committed baseline (`specs/003-eval-harness/baselines/lexical-baseline-v1.<d>.json`, `specs/004-dense-stage/baselines/dense-baseline-v1.<d>.json`, `specs/005-hybrid-pipeline/baselines/hybrid-baseline-v1.<d>.json`) and asserts that `target/xt-sparse-runs/<d>/engine-{lexical,dense,hybrid}.jsonl` exist and re-score through `reference()` to `mean_ndcg_10` / `mean_recall_100` within 1e-6 and equal `scored_queries` (SC-001) — **red now** (no runs exported)
- [X] T005 [P] Write `reference/tests_012/test_recipe.py`: imports `encode_documents`, `encode_queries`, `load_model` from `sparse_spike` (red: not defined); with the v3 manifest's model, encodes the model card's own example sentence and asserts the top-5 terms (decoded tokens) contain the words the card's example shows and every weight is > 0 and finite; asserts a document of 1,000 repeated words truncates (the tokenizer reports > max length) and encodes without error; asserts `encode_queries(["what is xtriever"])` produces exactly the query's distinct token ids with weights equal to `idf[token]` and no model call (patch `torch.nn.Module.__call__` to raise, or assert the function never receives the model)
- [X] T006 [P] Write `reference/tests_012/test_bm25x.py`: imports `bm25_over_field` from `sparse_spike` (red); a 3-document CSR with quantised weights `[[3,0,1],[0,2,0],[1,1,1]]` (terms t0..t2), query tokens `{t0, t2}`, k1 = 1.2, b = 0.75, IDF `ln(1 + (N − df + 0.5)/(df + 0.5))`; asserts the three scores against values computed by hand in the test (written out with the arithmetic), and that the ranking ties break by ascending doc id; a second case with `rrf` on two ranked lists `[a,b,c]`, `[c,a,d]`, k = 60 → order `[a, c, b, d]` with the fused scores stated
- [X] T007 Run the red checkpoint: `reference/.venv-012/bin/pytest reference/tests_012 -q` → `test_scorer` fails on the missing runs (the probe itself passes), `test_recipe` and `test_bm25x` fail on imports; record the counts in the report under "Red checkpoint". **⛔ Commit C1**: requirements, the two manifests, the script skeleton, the three tests, the report stub

---

## Phase 3: User Story 1 — Every variant gets a number on every dataset (Priority: P1)

**Goal**: `export`, `encode`, `score`, `all` for one model (v3) on the three sets — the metrics
table.

**Independent Test**: quickstart Steps 1–2 with the v3 manifest; `test_scorer` green; every
variant's report exists for scifact / nfcorpus / fiqa.

- [ ] T008 [US1] Implement `export --dataset D` in `reference/sparse_spike.py`: runs the harness for the three configs — `cargo run --release -p xtriever-eval --example beir -- run --dataset D --config lexical-baseline-v1 --export-run target/xt-sparse-runs/D/engine-lexical.jsonl`, `… --config dense-baseline-v1 --cache-dir target/xt-dense-cache --export-run …/engine-dense.jsonl`, `… --config hybrid-baseline-v1 --cache-dir target/xt-dense-cache --index-dir target/xt-rerank-index/D --export-run …/engine-hybrid.jsonl` (flags per `crates/xtriever-eval/examples/beir.rs:10–13`; add `--out /tmp/…json` so nothing under `specs/` is rewritten); then re-scores each with `reference()` against its committed baseline and **fails** (exit 1, naming the metric and the delta) unless within 1e-6 (SC-001); writes `engine-*.json` reports. Run for all three datasets → `test_scorer` green
- [ ] T009 [US1] Implement `load_model(manifest)`, `encode_documents(model, tokenizer, texts, activation, device, batch)` and `encode_queries(tokenizer, idf, texts)` in `reference/sparse_spike.py` per research D1: verify the model directory against the manifest's hashes before loading (refuse otherwise); `AutoModelForMaskedLM` from the local dir; document recipe = logits → `max over tokens of (output × attention_mask[..., None])` → `log1p(relu)` (v2) or `log1p(log1p(relu))` (v3) → zero `special_token_ids = [tokenizer.vocab[t] for t in tokenizer.special_tokens_map.values()]`; return CSR pieces (indices of non-zeros, f32 values); query recipe = `idf[token]` for each distinct token id of `tokenizer(text)["input_ids"]` excluding special tokens; count truncated documents (`len(tokenizer(text, truncation=False)["input_ids"]) > max_length`); the device chosen by `--device` (default: `mps` if available, else `cpu`), the batch by `--batch` (default 32); document text = `title + " " + text` for BEIR corpora (the harness's passage shape — check `crates/xtriever-eval/src/run.rs` for the exact join and match it) → `test_recipe` green
- [ ] T010 [US1] Implement `encode --manifest M --dataset D` (research D4): corpus in file order in shards of 1,000 → `target/xt-sparse-cache/<model-key>/D/docs-NNNNN.npz` (`ids`, `indptr`, `indices` int32, `data` f32; `.part` → rename; skip complete shards, print "cached"), judged queries (those in `qrels/test.tsv`) → `queries.npz`; per shard and per corpus: wall time, docs/s, device, `torch.get_num_threads()`, nnz per doc (mean, p95, max), truncated count; per query set: nnz mean/p95/max, empty count → appended to `target/xt-sparse-runs/D/costs-<model-key>.json` (data-model "Costs record"); run on scifact first and record docs/s on MPS vs CPU for the first shard (choose the faster for the rest and say which in the report)
- [ ] T011 [US1] Implement `score` variants `dot` and `dot-qS` in `reference/sparse_spike.py` (research D5): load the shards into one `scipy.sparse.csr_matrix` (N × vocab), queries into another; `scores = D @ q` per query; top-100 with `numpy.lexsort((doc_ids, -scores))`, drop zeros, write `target/xt-sparse-runs/D/<name>.jsonl` (ascending query ids, the harness's shape) and the `reference()` report as `<name>.json` with `provenance {model_key, variant, scale, params}`; `dot-q10/100/1000` quantise `D.data = round(D.data × S) / S` first
- [ ] T012 [US1] Implement `bm25_over_field(csr_tf, query_token_ids, k1=1.2, b=0.75)` and the variants `bm25x` (tf = `round(d × S)`, S = 100 unless `--scale`; query = the distinct query token ids) and `bm25x+text-bβ` (the spike's text BM25: lower-case, split on non-alphanumerics, Snowball English stemmer, no stop words, over `title + " " + text`; score = text BM25 + β × expansion BM25 for β ∈ {0.5, 1.0, 2.0}) in `reference/sparse_spike.py`; also `rrf(lists, k=60)`; report the rank-10 overlap between the spike's text-BM25 run and `engine-lexical.jsonl` (mean Jaccard of the top-10 sets) as the tokenizer-difference figure → `test_bm25x` green
- [ ] T013 [US1] Implement the `rrf-*` variants (`rrf-lex+dot`, `rrf-dense+dot`, `rrf-lex+dense+dot`, `rrf-lex+bm25x`, `rrf-dense+bm25x`, `rrf-lex+dense+bm25x`) reading `engine-lexical.jsonl` / `engine-dense.jsonl` and the spike's runs at depth 100, and `all --manifest M --dataset D` = encode (if needed) + every variant in a fixed order, printing one line per report with `ndcg_10` and `recall_100`; run `all` for v3 on scifact, then nfcorpus, then fiqa (quickstart Step 2). **⛔** If `dot` on SciFact is more than 3 nDCG points under the card's 0.708, stop: re-check the recipe against the card (activation, special tokens, passage join, truncation) and record what was found before continuing

---

## Phase 4: User Story 2 — The costs are measured (Priority: P1)

**Goal**: The quantisation table, the costs table, the Wikipedia projection.

**Independent Test**: `runs/costs-<v3-key>.json` complete; the quantisation rows for the three
datasets in the report.

- [ ] T014 [US2] Quantisation table: from the `dot` and `dot-q10/100/1000` reports per dataset, compute the nDCG@10 loss per scale; name the smallest scale within 0.1 points per dataset (SC-003) or state that none is; add the `quantisation` block to `specs/012-sparse-spike/runs/summary.json` via `summary` (T017) — implement the computation in `summary` now and print it
- [ ] T015 [US2] Costs and projection: from the per-dataset costs records compute, for the report, docs/s (FiQA — the largest — is the headline figure, SC-004), the model's bytes on disk (from the manifest), parameters, licence, revision, query-side requirement ("tokenizer + idf.json, N entries"); project Wikipedia: `postings = mean nnz per doc (FiQA) × 427,947`; bytes ≈ postings × 2 B (the tantivy posting-with-tf range 1.5–2.5 B stated as the literature's, research D6) → `specs/012-sparse-spike/runs/costs-<model-key>.json` (copied from `target/` with the projection added)

---

## Phase 5: User Story 3 — The second model, compared on equal terms (Priority: P2)

**Goal**: v2-distill through the same steps; the model table with licences.

**Independent Test**: every variant × dataset has a v2 row beside the v3 row.

- [ ] T016 [US3] Run `all` for `reference/models/manifest-sparse-doc-v2.json` on scifact, nfcorpus, fiqa (activation `log1p_relu`); copy its costs record to `specs/012-sparse-spike/runs/costs-<v2-key>.json`; add the model table to the report: repository, revision, licence, parameters, bytes, activation, query-side requirement — plus one row for the naver `splade-*` family marked **ineligible (CC BY-NC-SA 4.0), not run**, and one row quoting a published symmetric-SPLADE ceiling labelled "literature, not run" (Q1 = A)

---

## Phase 6: Polish — summary, report, gate

- [ ] T017 Implement `summary` in `reference/sparse_spike.py`: walks `target/xt-sparse-runs/*/*.json`, builds `specs/012-sparse-spike/runs/summary.json` (`{dataset: {variant@model: {ndcg_10, recall_100, scored_queries}}}` plus `engine-*`), prints the markdown tables (metrics per dataset with every variant for both models and the engine's three runs; quantisation; costs) and applies FR-011: **go** iff the recommended model's `dot` nDCG@10 > `engine-lexical` on ≥ 2 datasets **and** mean over the three of `rrf-lex+dense+dot` > mean of `engine-hybrid`; recommends `bm25x` over `dot` only if within 0.5 points; prints the verdict paragraph
- [ ] T018 Write `specs/012-sparse-spike/report.md`: Pins; Red checkpoint; the oracle agreement (SC-001 deltas); the metrics table (all variants × 3 datasets × 2 models, engine rows first); the quantisation table; the costs table and the Wikipedia projection; the model table; the tokenizer-difference figure (T012); findings (anything that deviated from the cards — e.g. the SciFact gap if any, the MPS-vs-CPU choice, truncation counts, empty queries); **the verdict** in one paragraph 013's spec can adopt verbatim (model revision, scoring, scale); "Deliberately not done" (research D7)
- [ ] T019 Gate and PR text: `git diff --stat main -- crates/ swift/ apps/ python/ .github/ deny.toml` empty; `cargo nextest run --workspace` unchanged (263 / 263); `reference/.venv-012/bin/pytest reference/tests_012 -q` green; no model weights, no `target/` files, no device/team identifiers in the tree; write `specs/012-sparse-spike/pr-description.md` (written for the reviewer: what was measured, the table's headline rows, the verdict, what is committed vs cached). **⛔ Commit C3**; the owner merges

---

## Dependencies & Execution Order

- **Phase 1**: T001 → T002 → T003 (the manifests need the script's `pin`).
- **Phase 2**: T004 ‖ T005 ‖ T006 (three files) → T007 (C1).
- **Phase 3**: T008 first (the oracle agreement gates everything); T009 → T010 → T011 → T012 → T013 (one file, sequential; the runs are hours on FiQA).
- **Phase 4**: T014 ‖ T015 after T013.
- **Phase 5**: T016 after T013 (the script is complete); can run while T014/T015 are written.
- **Phase 6**: T017 → T018 → T019.

### User story completion order

US1 (the table for v3) → US2 (costs) ‖ US3 (v2) → Polish.

### Parallel opportunities

T004 ‖ T005 ‖ T006; T014 ‖ T015; T016's long runs ‖ the Phase 4 writing.

## Implementation Strategy

**MVP** = Phases 1–3 for v3 on SciFact alone (the smoke: minutes): if `dot` is nowhere near
the card's number, the recipe is wrong and nothing else should run until it is right (T013's
stop-point). Then the two larger sets, then v2, then the tables.

**Rule 6 stop-points**: T007 (red must be red), T008 (any baseline not reproduced to 1e-6 —
the harness and the scorer must agree before a single variant counts), T013 (a recipe gap),
T019 (any gate failure). The decision rule is not touched after the runs.
