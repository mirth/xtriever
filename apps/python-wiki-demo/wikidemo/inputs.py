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
    "embedder": "reference/models/all-MiniLM-L6-v2-q8",
    "reranker": "reference/models/ms-marco-MiniLM-L-6-v2-q8",
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
    "embedder": "scripts/fetch-model.sh --manifest reference/models/manifest-q8.json",
    "reranker": "scripts/fetch-model.sh --manifest reference/models/manifest-rerank-q8.json",
    "chonky": "scripts/fetch-model.sh --manifest reference/models/manifest-chonky.json",
    "snapshot": "scripts/fetch-wiki.sh",
    "manifest": "git checkout -- reference/datasets/wiki-manifest.json   (it is in the tree)",
    "expected": (
        "cargo run --release -p xtriever-cli -- wiki expected --index target/xt-wiki/index "
        "--embedder-dir reference/models/all-MiniLM-L6-v2-q8 --reranker-dir reference/models/ms-marco-MiniLM-L-6-v2-q8 "
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


def weights_files(model_dir: Path) -> list[Path]:
    """Every file in `model_dir` the engine would take for weights: the float
    `model.safetensors` and each `*.gguf`, in that order."""
    found = []
    if (model_dir / "model.safetensors").exists():
        found.append(model_dir / "model.safetensors")
    if model_dir.is_dir():
        found.extend(sorted(model_dir.glob("*.gguf")))
    return found


def weights(model_dir: Path) -> Path | None:
    """The one weights file `model_dir` holds, or None when it holds none or several.

    The engine loads a directory holding exactly one — the float `model.safetensors` or the
    eight-bit GGUF (Feature 026) — and refuses one holding both, so neither a preference nor
    the first of several would be what it loads. It checks in addition that the file is the
    pinned artefact, by name and by hash, which nothing here can do.
    """
    found = weights_files(model_dir)
    return found[0] if len(found) == 1 else None


def unusable_weights(model_dir: Path) -> str | None:
    """Why a directory holding weights could not be loaded, or None. Holding none is not
    reported here: that is a missing input, named with the command that produces it."""
    found = weights_files(model_dir)
    if len(found) > 1:
        names = ", ".join(p.name for p in found)
        return f"{model_dir} holds {names}; a model directory holds exactly one weights file"
    return None


def _weights(model_dir: Path) -> Path:
    """`weights`, or — when the directory holds none — a path naming both forms, so the
    missing-input message never names a file the producer it recommends cannot create."""
    return weights(model_dir) or model_dir / "{model.safetensors,*.gguf}"


def _sentinel(paths: Paths, name: str) -> Path:
    return {
        "artefact": paths.index_dir / "xtriever-pipeline.json",
        "embedder": _weights(paths.embedder),
        "reranker": _weights(paths.reranker),
        "chonky": paths.chonky / "model.safetensors",
        "snapshot": paths.snapshot,
        "manifest": paths.manifest,
        "expected": paths.expected,
        "queries": paths.queries,
    }[name]


class MissingInput(Exception):
    def __init__(self, what: str, path: Path, producer: str):
        super().__init__(f"missing {what}: {path}")
        self.what, self.path, self.producer = what, path, producer


class UnusableInput(Exception):
    """An input that is present but the engine would refuse: a model directory holding more
    than one weights file. Reported here rather than left to fail at open, with what is wrong
    rather than a command that would not mend it."""

    def __init__(self, what: str, why: str):
        super().__init__(f"{what} is unusable: {why}")
        self.what, self.why = what, why


def _first_problem(paths: Paths, needs) -> Exception | None:
    """The first input in `needs` order that is absent or unusable, as the exception to raise."""
    for name in needs:
        if name in ("embedder", "reranker"):
            why = unusable_weights(getattr(paths, name))
            if why is not None:
                return UnusableInput(WHAT[name], why)
        path = _sentinel(paths, name)
        if not path.exists():
            return MissingInput(WHAT[name], path, PRODUCERS[name])
    return None


def first_missing(paths: Paths, needs) -> tuple[str, Path, str] | None:
    """`(what, path, producer)` for the first needed input that is absent, in `needs` order.

    A model directory that is present but unusable is not reported here; `require` reports it.
    """
    problem = _first_problem(paths, needs)
    if isinstance(problem, MissingInput):
        return problem.what, problem.path, problem.producer
    return None


def require(paths: Paths, needs) -> None:
    """Refuse before anything loads: the first needed input that is absent, named with the
    command that produces it, or the first that is present and unusable, named with what is
    wrong with it."""
    problem = _first_problem(paths, needs)
    if problem is not None:
        raise problem
