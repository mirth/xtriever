# Xtriever minimal demo (Python)

The smallest demonstration of the pipeline: ten short documents written in one file, an
index built from them through the `xtriever` package — create, add, commit, merge — and one
search shown twice: the **fused** stage (BM25 + dense embeddings, reciprocal-rank fusion),
then the **re-ranked** stage (the cross-encoder re-ordering the fused head). One file,
under 80 lines, nothing but the package and the two models. Feature 020
(`specs/020-minimal-python-example/`).

```bash
python apps/python-minimal-demo/demo.py "how do bees make honey"
python apps/python-minimal-demo/demo.py "why does the sea rise and fall"
```

## Inputs

| Input | Where | Produced by |
|---|---|---|
| a Python with the `xtriever` wheel | any venv (e.g. `apps/python-wiki-demo/.venv`) | `cd python && .venv/bin/maturin build --release`, then `uv pip install target/wheels/xtriever-*.whl` |
| the embedder | `reference/models/all-MiniLM-L6-v2` (`XTRIEVER_MODEL_DIR`) | `scripts/fetch-model.sh` |
| the re-ranker | `reference/models/ms-marco-MiniLM-L-6-v2` (`XTRIEVER_RERANK_MODEL_DIR`) | `scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json` |

The index is built into a temporary directory each run (a second or two for ten documents)
and removed afterwards; nothing is left on disk.

## What the output shows

```
indexed 10 documents

fused (lexical + dense), 5 hits
 1. doc-04  score=0.0328  Tides
 2. doc-02  score=0.0323  The water cycle
 3. doc-03  score=0.0312  Bread
 4. doc-05  score=0.0308  The Moon
 5. doc-08  score=0.0308  Coral reefs

re-ranked (depth 10), 5 hits
 1. doc-04  score=0.0328  rerank=8.6639  Tides
 2. doc-02  score=0.0323  rerank=-1.7712  The water cycle
 3. doc-09  score=0.0308  rerank=-9.8391  Thunderstorms
 4. doc-03  score=0.0312  rerank=-10.8310  Bread
 5. doc-08  score=0.0308  rerank=-10.3708  Coral reefs
```

"why does the sea rise and fall": the fused stage puts *Tides* first on both lexical and
dense evidence, then *The water cycle* and *Bread* (the word "rise"); the cross-encoder,
reading the question against each of the first ten fused candidates, keeps *Tides* and *The
water cycle*, pulls *Thunderstorms* (air that rises) into the head and drops *The Moon*.
`score` is the engine's fused score (RRF); `rerank` the cross-encoder's logit; the
re-ranked order is `0.5·minmax(fused) + 0.5·minmax(rerank)` over the head (the engine's
default since Feature 015).

## What is left out, and where it lives

- **Long documents and a real corpus** — cutting articles into passages that fit the
  embedder's window, building from a snapshot, the sidecars: `apps/python-wiki-demo`
  (`wikidemo build`).
- **Explanations, change marks and the stage report** — the eight per-hit features, what
  moved between the two lists, candidate counts and timings: `wikidemo search --explain`.
- **The phone**: `apps/ios-wiki-demo`.
- **A persistent index, an output directory, flags**: `wikidemo`.

## Tests

```bash
apps/python-wiki-demo/.venv/bin/pytest apps/python-minimal-demo/tests -q
```

Four model-free tests (the 80-line budget, the usage exit, the printed lines, the corpus's
shape) and one with the models: the script's hits equal a direct use of the package on the
same documents — ids in order and identical score bits at both depths — and a second run
gives the same result.
