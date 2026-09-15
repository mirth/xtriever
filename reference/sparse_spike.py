#!/usr/bin/env python
"""Feature 012 — the sparse-expansion spike (specs/012-sparse-spike).

Measures, in Python, what learned sparse expansions (OpenSearch's inference-free document
encoders) are worth in Xtriever's inverted index before any Rust is written: the engine's own
exported runs are the baselines, the 003 reference scorer is the oracle, and every variant of
research D5 becomes a run file with a report.

    sparse_spike.py pin      --repo R --out MANIFEST
    sparse_spike.py encode   --manifest M --dataset D [--device mps|cpu] [--batch 32]
    sparse_spike.py export   --dataset D
    sparse_spike.py score    --manifest M --dataset D --variant NAME [--scale 100] [--boost 1.0]
    sparse_spike.py all      --manifest M --dataset D
    sparse_spike.py summary

Nothing here ships: caches and runs live under target/, the manifests and the summary JSON
under the repository, the weights nowhere (scripts/fetch-model.sh fetches them by pin).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MODELS_DIR = REPO_ROOT / "reference" / "models"
BEIR_DIR = REPO_ROOT / "reference" / "datasets" / "beir"
CACHE_DIR = REPO_ROOT / "target" / "xt-sparse-cache"
RUNS_DIR = REPO_ROOT / "target" / "xt-sparse-runs"
SPEC_RUNS_DIR = REPO_ROOT / "specs" / "012-sparse-spike" / "runs"

MODEL_FILES = ["config.json", "tokenizer.json", "model.safetensors", "idf.json"]
DATASETS = ["scifact", "nfcorpus", "fiqa"]
DEPTH = 100
RRF_K = 60
BM25_K1, BM25_B = 1.2, 0.75
SHARD = 1_000


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# --------------------------------------------------------------------------------------------
# pin: the manifest in the 004 shape (research D1), fetched later by scripts/fetch-model.sh
# --------------------------------------------------------------------------------------------
def cmd_pin(args: argparse.Namespace) -> int:
    from huggingface_hub import HfApi, hf_hub_download

    api = HfApi()
    info = api.model_info(args.repo)
    revision = info.sha
    card = info.card_data or {}
    license_name = card.get("license") if isinstance(card, dict) else getattr(card, "license", None)
    basename = args.repo.split("/")[-1]
    activation = "log1p_log1p_relu" if "-v3-" in basename else "log1p_relu"
    files = []
    with tempfile.TemporaryDirectory() as tmp:
        for name in MODEL_FILES:
            local = Path(hf_hub_download(repo_id=args.repo, filename=name, revision=revision, cache_dir=tmp))
            files.append({"name": name, "bytes": local.stat().st_size, "sha256": sha256_file(local)})
            print(f"pin: {name} {files[-1]['bytes']} {files[-1]['sha256']}")
        # Parameter count from the safetensors header (the card says 67M for both models).
        st = next(p for p in Path(tmp).rglob("model.safetensors"))
        with st.open("rb") as f:
            n = int.from_bytes(f.read(8), "little")
            header = json.loads(f.read(n))
        params = 0
        for k, v in header.items():
            if k == "__metadata__":
                continue
            count = 1
            for d in v["shape"]:
                count *= d
            params += count
    manifest = {
        "schema_version": 1,
        "note": (
            "Feature 012 sparse-expansion spike: an inference-free document encoder (OpenSearch "
            "neural sparse), pinned by revision and file hash; fetched by scripts/fetch-model.sh, "
            "never committed. Query side: tokenizer + idf.json, no model call (research D1)."
        ),
        "repository": args.repo,
        "revision": revision,
        "local_dir": basename,
        "license": license_name,
        "parameters": params,
        "activation": activation,
        "files": files,
    }
    out = Path(args.out)
    out.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"pin: wrote {out} ({args.repo} @ {revision}, {params:,} parameters, license {license_name})")
    return 0


def not_implemented(args: argparse.Namespace) -> int:
    sys.exit(f"sparse_spike: `{args.command}` is not implemented yet")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)

    s = sub.add_parser("pin", help="write a pinned manifest for a model repository")
    s.add_argument("--repo", required=True)
    s.add_argument("--out", required=True)
    s.set_defaults(func=cmd_pin)

    for name in ("encode", "export", "score", "all", "summary"):
        s = sub.add_parser(name)
        s.add_argument("--manifest")
        s.add_argument("--dataset", choices=DATASETS)
        s.add_argument("--variant")
        s.add_argument("--device", choices=["mps", "cpu"])
        s.add_argument("--batch", type=int, default=32)
        s.add_argument("--scale", type=int, default=100)
        s.add_argument("--boost", type=float, default=1.0)
        s.set_defaults(func=not_implemented)

    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
