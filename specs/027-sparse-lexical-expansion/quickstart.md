# Quickstart: validating Feature 027

Run from the repository root on the build host. The encoder is 268 MB and only the build host
ever fetches it.

## 0. Artefacts

```bash
scripts/fetch-model.sh --manifest reference/models/manifest-sparse-doc-v3.json
scripts/fetch-beir.sh scifact nfcorpus fiqa
scripts/setup-reference-venv.sh 027          # PyTorch + transformers for the oracle
```

## 1. The oracle (PR A)

```bash
reference/.venv-027/bin/python reference/gen_027_fixtures.py            # writes reference/fixtures/027/
cargo nextest run -p xtriever-dense --run-ignored all -E 'binary(sparse_oracle)'
```

Expected: every fixture weight within 1e-4 of the reference, every term frequency at scale 10
equal apart from the boundary cases the fixture marks, every query's ids equal; the second run
on another thread count (`RAYON_NUM_THREADS=1`) byte-identical.

## 2. The index (PR B)

```bash
cargo nextest run -p xtriever-pipeline -E 'binary(sparse_index)'          # model-free: format, refusals, query shape
cargo nextest run -p xtriever-pipeline --run-ignored all -E 'binary(sparse_index)'
```

Expected: an option-off index byte-identical to today's; a sparse index at descriptor version 3
with `sparse/` holding the two pinned files; an altered `sparse/tokenizer.json` refused by hash;
`add` without the encoder refused; the query built as research D7 says.

## 3. The measurement (PR B)

```bash
for d in scifact nfcorpus fiqa; do
  cargo run --release -p xtriever-eval --example beir -- run --dataset $d --config hybrid-sparse-rerank-v1 \
    --sparse-encoder-dir reference/models/opensearch-neural-sparse-encoding-doc-v3-distill \
    --cache-dir target/xt-dense-cache --sparse-cache-dir target/xt-sparse-cache-027 \
    --index-dir target/xt-sparse-index/$d --out specs/027-sparse-lexical-expansion/runs/hybrid-sparse-rerank-v1.$d.json
done
```

The first run of FiQA encodes 57,638 documents: expect an overnight job on CPU. Expected, against
`hybrid-rerank-v3` (specs/026-eight-bit-precision/runs/): FiQA nDCG@10 at least +0.010 (SC-001);
no dataset below −0.005 on either metric (SC-002); the spike measured +0.0174 / −0.0003 / −0.0048.
A breach of SC-002 stops the feature and is reported (Rule 6). Re-run `hybrid-rerank-v3` and
`hybrid-baseline-v2` to confirm the option-off numbers are unchanged (SC-003).

## 4. The surfaces (PR C)

```bash
cargo run --release -p xtriever-cli -- wiki build --limit 200 --out target/xt-wiki-sparse-dev \
  --sparse-encoder reference/models/opensearch-neural-sparse-encoding-doc-v3-distill
python/.venv/bin/pytest python/tests -q -k sparse
```

Expected: a sparse slice built and searched through Python with the engine's own results; its
`info()` reporting the option; the Swift and Kotlin `IndexInfo` compiling with the new field; the
packagers' check passing (no sparse manifest named).
