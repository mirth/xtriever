# Data Model: Eight-Bit Precision End to End (Feature 026)

Phase 1. Two things change on disk: the dense row, and which file a model is loaded from.
Everything else — the manifest, the tombstones, the generations, the compaction protocol from
Feature 024 — is untouched.

## The dense row, format version 3

| Field | Type | Bytes | Meaning |
|---|---|---|---|
| `id` | `u32` | 4 | the internal document identifier, as today |
| `norm` | `f32` | 4 | the norm of the row **as stored**, `sqrt(Σ code²) × scale` — no longer of the floats, which are not kept |
| `scale` | `f32` | 4 | **new**: multiply a code by this to recover the component; a normal, positive number |
| `codes` | `i8 × dim` | dim | **new**: the quantised components, replacing `f32 × dim` |

At dimension 384 a row is 396 bytes against 1,544 today, a factor of 3.9. Rows stay fixed width,
which is what lets the file be mapped and indexed arithmetically.

**Invariants.**

- `scale` is `max|component| / 127` over that vector, floored at the smallest normal `f32`, and
  is never zero or denormal: a vector of all zeros stores a scale of one and codes of zero, so
  recovery gives back zeros rather than a division by zero, and a denormal peak is quantised at
  the floor rather than storing a scale no reader could tell from corruption.
- `code` is `round(component / scale)` with half-way cases away from zero (`f32::round`), clamped
  to [−127, 127]. The value −128 is never written, which keeps negation exact. The reference
  generator restates the same rounding; Python's `round`, which goes to even, would not do.
- `norm` is the norm of the recovered row, so cosine — the quantised dot product over the two
  quantised norms — is the cosine of what is compared: a row against itself scores one.
- Recovery is `component ≈ code × scale`; the engine never claims it is exact.

## The manifest header

Unchanged in shape. `format_version` becomes 3 and the header names the quantisation scheme
(`"scheme":"i8-symmetric-per-vector"`), so a future scheme is a header change rather than a
guess. A file whose version is 1 or 2 is refused
at open by name, with the rebuild instruction — the same refusal Feature 024 introduced.

## Scoring

| Step | What happens |
|---|---|
| the query | quantised with the same scheme, giving codes and one scale |
| the scan | `i32` accumulation of code products over the dimension |
| the score | accumulator × query scale × row scale, as `f32`; for cosine, divided first by the two quantised norms |
| the guard | a row whose scale is not a normal positive number, or whose norm cosine could not divide by, is refused as corrupt by the scan |

Determinism is unchanged: the same index, query and configuration produce the same score bits,
because nothing here depends on ordering or on a floating-point reduction order.

## The model artefacts

| | Float, today | Eight-bit, added |
|---|---|---|
| File | `model.safetensors` | a GGUF file naming architecture, block count, embedding length |
| Weight matrices | float32 | eight-bit blocks |
| Norms, biases, token types | float32 | float32 |
| Tokenizer | `tokenizer.json` beside the weights | **the same file**; the artefact supplies weights only |
| Pinned by | repository, revision, per-file checksum | the same |
| Fingerprint | names the float artefact | names the eight-bit artefact |

**The re-ranker's head.** A cross-encoder needs a classification layer to turn a pooled
representation into a relevance score. The pinned artefact carries `classifier.weight` and
`classifier.bias`; a file without them is refused at load, because it would otherwise produce
embeddings that look like scores.

## What an index records

Unchanged in shape and meaning: the embedder's fingerprint, the format version, the dimension,
the metric. What changes is their values. An index built by this feature cannot be opened with
the float embedder, and an index built before it cannot be opened at all — both are hard errors
at open, named, never a silent reinterpretation.

## What has to be regenerated

| Artefact | Why |
|---|---|
| the Wikipedia corpus | format 3 and a different embedder |
| its host goldens and measurement records | different scores |
| the 40-document fixture index and its goldens | the same |
| the demo corpus slices | the same |
| the evaluation caches for all three datasets | a different embedder invalidates every cached vector |
| the dense stage's reference oracle | a new row layout |
