"""Where the inputs are and what produces them (research D11; contracts/cli.md).

Every path resolves flag → environment variable → default, relative paths against the
repository root. A command checks the inputs it needs *before* loading a model, and reports
the first missing one with the command that produces it.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

ENV = {
    "artefact": "XTRIEVER_WIKI_ARTEFACT",
    "embedder": "XTRIEVER_MODEL_DIR",
    "reranker": "XTRIEVER_RERANK_MODEL_DIR",
    "chonky": "XTRIEVER_CHONKY_MODEL_DIR",
}

DEFAULTS = {
    "artefact": "target/xt-wiki",
    "embedder": "reference/models/all-MiniLM-L6-v2",
    "reranker": "reference/models/ms-marco-MiniLM-L-6-v2",
    "chonky": "reference/models/chonky_distilbert_base_uncased_1",
    "snapshot": "reference/datasets/wiki/simple.jsonl",
    "manifest": "reference/datasets/wiki-manifest.json",
    "queries": "reference/fixtures/008/queries.json",
}

PRODUCERS = {
    "package": "cd python && .venv/bin/maturin build --release && uv pip install ../target/wheels/xtriever-*.whl",
    "artefact": (
        "cargo run --release -p xtriever-cli -- wiki build --out target/xt-wiki   "
        "(the full corpus, hours) or: wikidemo build --limit N --out DIR   (a slice, minutes)"
    ),
    "embedder": "scripts/fetch-model.sh",
    "reranker": "scripts/fetch-model.sh --manifest reference/models/manifest-rerank.json",
    "chonky": "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json",
    "snapshot": "scripts/fetch-wiki.sh",
    "manifest": "git checkout -- reference/datasets/wiki-manifest.json   (it is in the tree)",
    "expected": (
        "cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index "
        "--embedder-dir reference/models/all-MiniLM-L6-v2 --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2 "
        "--queries reference/fixtures/008/queries.json --out target/xt-wiki/expected.json"
    ),
    "queries": "git checkout -- reference/fixtures/008/queries.json   (it is in the tree)",
}

WHAT = {
    "artefact": "the Wikipedia artefact",
    "embedder": "the embedder",
    "reranker": "the re-ranker",
    "chonky": "the chonky splitter model",
    "snapshot": "the snapshot",
    "manifest": "the snapshot manifest",
    "expected": "the host goldens",
    "queries": "the measurement queries",
}


def repo_root() -> Path:
    """The checkout this demo lives in: the nearest ancestor holding `Cargo.toml` and `.specify/`."""
    here = Path(__file__).resolve()
    for candidate in here.parents:
        if (candidate / "Cargo.toml").exists() and (candidate / ".specify").is_dir():
            return candidate
    return here.parents[2]


@dataclass(frozen=True)
class Paths:
    artefact: Path
    embedder: Path
    reranker: Path
    chonky: Path
    snapshot: Path
    manifest: Path
    queries: Path
    expected: Path

    @property
    def index_dir(self) -> Path:
        return self.artefact / "index"

    @property
    def corpus_json(self) -> Path:
        return self.index_dir / "corpus.json"

    @property
    def attribution(self) -> Path:
        return self.artefact / "ATTRIBUTION.txt"


def _pick(flag, env_name, default, root: Path) -> Path:
    value = flag
    if value is None and env_name is not None:
        value = os.environ.get(env_name) or None
    if value is None:
        value = default
    path = Path(value)
    return path if path.is_absolute() else root / path


def resolve(args) -> Paths:
    """Resolve every input from the parsed arguments (attributes may be missing or None)."""
    root = repo_root()
    get = lambda name: getattr(args, name, None)  # noqa: E731
    artefact = _pick(get("artefact"), ENV["artefact"], DEFAULTS["artefact"], root)
    expected = get("expected")
    return Paths(
        artefact=artefact,
        embedder=_pick(get("embedder"), ENV["embedder"], DEFAULTS["embedder"], root),
        reranker=_pick(get("reranker"), ENV["reranker"], DEFAULTS["reranker"], root),
        chonky=_pick(get("chonky"), ENV["chonky"], DEFAULTS["chonky"], root),
        snapshot=_pick(get("snapshot"), None, DEFAULTS["snapshot"], root),
        manifest=_pick(get("manifest"), None, DEFAULTS["manifest"], root),
        queries=_pick(get("queries"), None, DEFAULTS["queries"], root),
        expected=_pick(expected, None, artefact / "expected.json", root),
    )


def _sentinel(paths: Paths, name: str) -> Path:
    return {
        "artefact": paths.index_dir / "xtriever-pipeline.json",
        "embedder": paths.embedder / "model.safetensors",
        "reranker": paths.reranker / "model.safetensors",
        "chonky": paths.chonky / "model.safetensors",
        "snapshot": paths.snapshot,
        "manifest": paths.manifest,
        "expected": paths.expected,
        "queries": paths.queries,
    }[name]


def first_missing(paths: Paths, needs) -> tuple[str, Path, str] | None:
    """`(what, path, producer)` for the first needed input that is absent, in `needs` order."""
    for name in needs:
        path = _sentinel(paths, name)
        if not path.exists():
            return WHAT[name], path, PRODUCERS[name]
    return None


class MissingInput(Exception):
    def __init__(self, what: str, path: Path, producer: str):
        super().__init__(f"missing {what}: {path}")
        self.what, self.path, self.producer = what, path, producer


def require(paths: Paths, needs) -> None:
    missing = first_missing(paths, needs)
    if missing is not None:
        raise MissingInput(*missing)
