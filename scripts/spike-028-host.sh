#!/usr/bin/env bash
# Feature 028, the accelerated inference spike: the host's measurements, one command per path.
# Spike code — removed or promoted by the follow-up the spike's verdict names (spec FR-012).
#
#     scripts/spike-028-host.sh <cpu|accelerate|metal> [--batch] <measure|build|mixed|quality>
#
#   measure  build the wheel with the path's features into the Python demo's environment and run
#            `wikidemo measure` on the Wikipedia artefact with the same labels; the record goes to
#            specs/028-accelerated-inference-spike/runs/<machine>-<path>-<single|batch>-measure-<stamp>.json
#   build    `beir run --dataset scifact --config dense-baseline-v1` with the path's features and a
#            fresh --cache-dir target/spike-028/<path> (the configuration that embeds the corpus
#            into the cache), then again warm; the difference is the corpus's embedding time
#            (data-model "Build measurement") → runs/build-<path>.json. Then `hybrid-rerank-v3`
#            once, which builds the hybrid index from those vectors (the mixed case reuses it)
#   mixed    the CPU path searching the Metal-built SciFact cache (research D10); run `metal build`
#            first → runs/mixed-scifact.json
#   quality  `beir run` on SciFact, NFCorpus and FiQA with the path → runs/<path>.hybrid-rerank-v3.
#            <dataset>.json, and `beir delta` against Feature 026's CPU records
#
# The path's features are compile-time (research D5); the label each record carries comes from
# the same arguments as the features, so the two cannot disagree. After `measure`, the demo's
# environment holds a spike wheel: `scripts/check-demos.sh python` reinstalls the default one.

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
runs="specs/028-accelerated-inference-spike/runs"
mkdir -p "$runs"

usage() { echo "usage: scripts/spike-028-host.sh <cpu|accelerate|metal> [--batch] <measure|build|mixed|quality>" >&2; exit 1; }
[ $# -ge 2 ] || usage
path="$1"; shift
case "$path" in cpu|accelerate|metal) ;; *) usage ;; esac
batch=false
if [ "${1:-}" = --batch ]; then batch=true; shift; fi
[ $# -eq 1 ] || usage
command="$1"

machine="$(sysctl -n hw.model)"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mode="$([ "$batch" = true ] && echo batch || echo single)"
now() { python3 -c 'import time; print(time.time())'; }

# The FFI crate's features for the wheel; the model crates' for the eval example.
ffi_features=()
model_features=()
if [ "$path" != cpu ]; then
    ffi_features+=("spike-$path")
    model_features+=("xtriever-dense/spike-$path" "xtriever-rerank/spike-$path")
fi
if [ "$batch" = true ]; then
    ffi_features+=("spike-batch")
    model_features+=("xtriever-rerank/spike-batch")
fi
join() { local IFS=,; echo "$*"; }
beir() {
    local flags=()
    [ ${#model_features[@]} -gt 0 ] && flags=(--features "$(join "${model_features[@]}")")
    cargo run -q --release -p xtriever-eval --example beir ${flags[@]+"${flags[@]}"} -- "$@"
}

case "$command" in
    measure)
        venv=apps/python-wiki-demo/.venv
        rm -f target/wheels/xtriever-*.whl
        flags=()
        [ ${#ffi_features[@]} -gt 0 ] && flags=(--features "$(join "${ffi_features[@]}")")
        (cd python && .venv/bin/maturin build --release ${flags[@]+"${flags[@]}"})
        uv pip install --python "$venv/bin/python" --force-reinstall target/wheels/xtriever-*.whl
        label=(--compute-path "$path")
        [ "$batch" = true ] && label+=(--rerank-batch)
        out="$runs/$machine-$path-$mode-measure-$stamp.json"
        "$venv/bin/wikidemo" measure "${label[@]}" --out "$out"
        echo "spike-028-host: measure $path $mode → $out"
        ;;
    build)
        cache="target/spike-028/$path"
        rm -rf "$cache"
        t0="$(now)"; beir run --dataset scifact --config dense-baseline-v1 --cache-dir "$cache" >/dev/null
        t1="$(now)"; beir run --dataset scifact --config dense-baseline-v1 --cache-dir "$cache" >/dev/null
        t2="$(now)"
        beir run --dataset scifact --config hybrid-rerank-v3 --cache-dir "$cache" >/dev/null
        python3 - "$path" "$t0" "$t1" "$t2" "$runs/build-$path.json" <<'EOF'
import json, sys
path, t0, t1, t2, out = sys.argv[1], *map(float, sys.argv[2:5]), sys.argv[5]
fresh, warm = t1 - t0, t2 - t1
embed = fresh - warm
record = {
    "feature": "028-accelerated-inference-spike", "path": path, "dataset": "scifact", "passages": 5183,
    "freshSeconds": round(fresh, 1), "warmSeconds": round(warm, 1), "embedSeconds": round(embed, 1),
    "passagesPerSecond": round(5183 / embed, 2),
    "projectedWikipediaHours": round(427947 / (5183 / embed) / 3600, 2),
    "note": "embedSeconds = a fresh-cache dense-baseline-v1 run minus the same run warm; wall clock, threads = candle's default",
}
json.dump(record, open(out, "w"), indent=2); open(out, "a").write("\n")
print(json.dumps(record))
EOF
        ;;
    mixed)
        [ "$path" = cpu ] || { echo "mixed runs the cpu path against the metal cache" >&2; exit 1; }
        [ -d target/spike-028/metal ] || { echo "run 'scripts/spike-028-host.sh metal build' first" >&2; exit 1; }
        beir run --dataset scifact --config hybrid-rerank-v3 --cache-dir target/spike-028/metal --out "$runs/mixed-scifact.json"
        beir delta specs/026-eight-bit-precision/runs/hybrid-rerank-v3.scifact.json "$runs/mixed-scifact.json"
        ;;
    quality)
        for dataset in scifact nfcorpus fiqa; do
            out="$runs/$path.hybrid-rerank-v3.$dataset.json"
            beir run --dataset "$dataset" --config dense-baseline-v1 --cache-dir "target/spike-028/$path" >/dev/null
            beir run --dataset "$dataset" --config hybrid-rerank-v3 --cache-dir "target/spike-028/$path" --out "$out"
            beir delta "specs/026-eight-bit-precision/runs/hybrid-rerank-v3.$dataset.json" "$out"
        done
        ;;
    *) usage ;;
esac
