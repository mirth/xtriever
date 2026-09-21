# Research: Eight-Bit Precision End to End (Feature 026)

Phase 0. Every decision below rests on something measured under `reference/` before the spec was
written, on the two artefacts inspected on 2026-09-20, or on the pinned engine's own source read
at its pinned version. Where a number appears, it was measured on this machine.

## D1 — The quantisation scheme: symmetric, one scale per vector

**Decision**: each stored vector becomes one signed byte per dimension plus a float scale, with
`scale = max|component| / 127` and `code = round(component / scale)`.

**Rationale**: measured on two datasets. Against the exact float ranking, per-vector scaling
costs 0.0006 nDCG@10 on SciFact (0.6451 → 0.6445) and 0.0006 on NFCorpus (0.3159 → 0.3153), with
Recall@100 unchanged on both. The candidate set a fusion stage receives agrees 99.5 % at depth
100, the top hit is unchanged for 99.7 % of queries, and the mean score error is 4.2e-4 on scores
that live in [−1, 1].

**Alternatives considered**: a single global scale measured marginally better (0.6449 and 0.3157)
and saves four bytes a row, but it ties every future corpus to one distribution — a corpus whose
vectors are less uniform would quantise worse with no way to notice. Asymmetric quantisation with
a zero point buys nothing here because the vectors are L2-normalised and centred. Four-bit was
not measured and is not proposed.

## D2 — No rescoring pass, and what that costs

**Decision**: the float vectors are not retained; the eight-bit scores are final.

**Rationale**: the owner's decision on 2026-09-20, taken against the measurement. A rescoring
pass over the top twenty recovers the exact float ranking — that was measured too — but it
requires keeping the float rows on disk, which grows the shipped corpus from 661 MB to 830 MB.
The owner chose the smaller file and accepted 0.0006 nDCG@10, an eighth of the threshold FR-009
sets for the whole feature.

**Consequence worth stating**: spec FR-002 and SC-001 had to loosen from "identical ranking" to
"within the measured thresholds", because without float vectors there is nothing to be identical
to. The determinism requirement is untouched: the same index, query and configuration still
produce the same scores.

## D3 — The row layout

**Decision**: `id u32 · norm f32 · scale f32 · codes dim×i8` — 396 bytes at dimension 384,
against 1,544 today.

**Rationale**: it keeps the two fields the format already has, so the manifest, the tombstones,
the generations and the compaction protocol from Feature 024 all work unchanged; only the row
body changes. Fixed-width rows are what lets the stage map the file and index it arithmetically,
which the append-and-compact design depends on.

## D4 — The scoring kernel

**Decision**: quantise the query with the same scheme, accumulate the dot product in `i32`, and
dequantise once per row by multiplying the two scales.

**Rationale**: it is what the study measured, so the quality numbers describe the kernel we will
ship rather than a different one. Integer accumulation of 384 terms of at most 127 × 127 cannot
overflow `i32` by four orders of magnitude, so no saturation logic is needed.

**On speed**: a probe outside the repository scanned a Wikipedia-sized matrix in 3.7–7.8 ms
against 16 ms for a fair float kernel, on a quarter of the memory traffic. The owner waived a
kernel probe against the engine's own code, so **this plan claims no speed improvement**; the
benchmark records what happens, and the feature's justification is size and quality.

## D5 — Reading the eight-bit artefacts

**Decision**: read the GGUF file with the pinned engine's own reader and build the forward pass
from quantised tensors: `quantized::gguf_file::Content::read` for the file, `QMatMul::from_qtensor`
for every weight matrix, and `QTensor::dequantize` for the pieces that are not matrix multiplies.

**Rationale**: the engine already has all of it, including the eight-bit block format the two
artefacts use. What it does not have is a quantised BERT — its twenty-two quantised models are
all decoder-style language models — but both of our crates already write their own forward pass,
so what changes is which multiply they call, not the shape of the computation.

**What the artefacts contain** (inspected 2026-09-20): architecture `bert`, six blocks, embedding
length 384, context length 512; 37 eight-bit matrices in the embedder and 38 in the re-ranker,
with layer norms, biases and token-type embeddings left in float. The re-ranker carries
`classifier.weight` and `classifier.bias`, which is what makes it a re-ranker rather than an
embedder; FR-008 requires checking for them, because a file without them would silently produce
the wrong kind of number.

## D6 — The tokenizer stays where it is

**Decision**: the tokenizer keeps coming from the pinned float model directory.

**Rationale**: neither eight-bit repository ships a `tokenizer.json` — the embedder's has only a
configuration file beside the weights. The tokenizer is not affected by weight precision, and the
existing one is already pinned by checksum, so the eight-bit artefact supplies weights only.

**Amended 2026-09-21, the re-ranker's pooler.** Implementation found the pinned re-ranker file
carries `classifier.weight` and `classifier.bias` — identical to the float head, checked byte for
byte — but no `bert.pooler.dense`, and declares mean pooling. The published model scores
`classifier(tanh(pooler(CLS)))`, so the artefact cannot reproduce it alone. The owner chose to
borrow the pooler from the pinned float weights the way the tokenizer is borrowed: cut into
`pooler.safetensors` by `scripts/extract_tensors.py` (a byte copy, no conversion), pinned in
`manifest-rerank-q8.json` under `borrows.tensors`, staged by the fetch script, and named in the
identity string.

## D7 — Pinning and fingerprints

**Decision**: pin each artefact in the model manifest by repository, revision and per-file
checksum, exactly as the float weights are pinned, and extend each fingerprint to name the
artefact it was loaded from.

**Rationale**: the fingerprint is what makes an index self-describing, and an index built with
eight-bit weights is not the same index. Naming the artefact means a mismatch is caught at open
instead of producing quietly different results. It also keeps the owner's rule intact: weights
enter this project as pinned upstream artefacts, never as something converted here.

**Provenance note**: both files are third-party conversions rather than publications by the model
authors. The embedder's repository has around 415,000 downloads, the re-ranker's around 1,100.
That is not a reason to refuse them, but it is a reason the quality gate runs on all three
datasets before anything ships.

## D8 — Both artefact formats stay loadable

**Decision**: float and eight-bit both load; the manifest decides which an installation uses.

**Rationale**: the owner's decision. It keeps a fallback if an eight-bit artefact disappoints on
a corpus we have not measured, at the cost of two loading paths and two fingerprint families in
the tests. It also shapes the loader the way the follow-up feature needs: reading a model the
file describes, rather than verifying known bytes.

## D9 — What is deliberately not done

No four-bit anything. No accelerator: this stays on the pinned engine and the processor. No
rescoring pass (D2). No arbitrary user-supplied models — the loader work here is most of what
that needs, and the spec records it as the next feature, but the quality guarantee cannot follow
an unknown model and that deserves its own specification. No continuous-integration change: the
three-dataset evaluation stays local.
