# Contract: The Spike's Surface (Feature 028)

Everything the spike adds that a person or a script invokes. All of it is off by default and
labelled `spike`; the follow-up removes or promotes it (spec FR-012).

## Cargo features

| crate | feature | effect |
|---|---|---|
| `xtriever-dense` | `spike-accelerate` | `candle-core/accelerate`, `candle-nn/accelerate`, `candle-transformers/accelerate`; device stays CPU |
| `xtriever-dense` | `spike-metal` | `candle-core/metal`, `candle-nn/metal`, `candle-transformers/metal`; the embedder is built on `Device::new_metal(0)` |
| `xtriever-rerank` | `spike-accelerate`, `spike-metal` | the same, for the cross-encoder |
| `xtriever-rerank` | `spike-batch` | `rerank` scores all pairs of a call in one forward pass (research D7) |
| `xtriever-ffi` | `spike-accelerate`, `spike-metal`, `spike-batch` | forward to both model crates (the batch to the re-ranker only) |

- `spike-accelerate` with `spike-metal` in one crate is a compile error naming both.
- Each model crate exposes `spike::COMPUTE_PATH: &str` (`"cpu"`, `"accelerate"` or `"metal"`),
  equal to the active feature; a test pins it.
- A Metal device that cannot be opened fails the model's `load` with `Error::Model` naming the
  spike path and candle's message. No fallback (spec).
- Identity is unchanged under every feature: `Embedder::fingerprint()` and the re-ranker's
  `model_id()` are today's strings (research D6).
- The sparse encoder is not affected by any spike feature.

## iOS packager

```
scripts/build-ios-package.sh … [--spike-compute accelerate|metal] [--spike-batch]
```

- Builds the XCFramework with the matching `xtriever-ffi` features and stages
  `XtrieverData/compute-path.json`: `{"computePath": "…", "rerankBatch": …}`. Without either flag
  it stages `{"computePath": "cpu", "rerankBatch": false}` — so every record from now on names its
  path — and builds exactly today's framework.
- Prints the `OTHER_LDFLAGS` the app's build needs for the chosen path (research D9):
  `-framework Accelerate`, or `-framework Metal -framework Foundation -framework CoreGraphics`.

## Host script

```
scripts/spike-028-host.sh <cpu|accelerate|metal> [--batch] [measure|build|mixed|quality]
```

- `measure`: builds the wheel with the matching features into the Python demo's environment and
  runs `wikidemo measure` with `--compute-path` and `--rerank-batch` set to the same values.
- `build`: `beir run --dataset scifact --config hybrid-rerank-v3` with the features, a fresh
  `--cache-dir target/spike-028/<path>`, then again warm; prints the build measurement
  (data-model "Build measurement").
- `mixed`: the CPU path against `target/spike-028/metal` (D10).
- `quality`: `beir run` on SciFact, NFCorpus and FiQA with the path, `--out` into
  `specs/028-accelerated-inference-spike/runs/`, then `beir delta` against the committed CPU
  records.

## Harness additions

| harness | adds | input |
|---|---|---|
| Swift package `DeviceMeasurementTests` | `computePath`, `rerankBatch`, `hitsDigest` | the staged `compute-path.json` |
| iOS demo `DemoMeasurementTests` | `computePath`, `rerankBatch`, `hitsDigest` | the staged `compute-path.json` |
| Python demo `wikidemo measure` | `computePath`, `rerankBatch`, `hitsDigest` | `--compute-path`, `--rerank-batch` (defaults `cpu`, `false`) |

A missing `compute-path.json` on the phone records `"unknown"`, never a guess.
