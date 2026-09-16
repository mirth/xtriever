"""Opening the artefact and running one search as two engine calls — the fused stage, then
the re-ranked stage — exactly as the iOS demo does (research D2). Wall clocks wrap the calls;
everything else is the engine's.
"""

from __future__ import annotations

import time
from dataclasses import dataclass

import xtriever

from .inputs import Paths
from .record import peak_resident_bytes


@dataclass
class Opened:
    handle: xtriever.IndexHandle
    paths: Paths
    info: xtriever.IndexInfo
    open_ms: int


def open_artefact(paths: Paths) -> Opened:
    """Open `paths.index_dir` with both models memory-mapped; the wall clock around the open."""
    t = time.perf_counter()
    handle = xtriever.IndexHandle.open(
        str(paths.index_dir), str(paths.embedder), str(paths.reranker), xtriever.LoadPath.MMAP
    )
    open_ms = round((time.perf_counter() - t) * 1000)
    return Opened(handle=handle, paths=paths, info=handle.info(), open_ms=open_ms)


@dataclass
class StageRun:
    label: str  # "fused" | "re-ranked"
    options: xtriever.SearchOptions
    response: xtriever.SearchResponse
    wall_ms: int
    peak_bytes: int


def timed_search(handle, label: str, query: str, options: xtriever.SearchOptions) -> StageRun:
    t = time.perf_counter()
    response = handle.search(query, options)
    wall_ms = round((time.perf_counter() - t) * 1000)
    return StageRun(label=label, options=options, response=response, wall_ms=wall_ms, peak_bytes=peak_resident_bytes())


def rerank_mode_option(mode: str):
    """The engine option for `--mode`: None keeps the index's recorded mode (interpolate)."""
    return xtriever.RerankMode.REPLACE() if mode == "replace" else None


def run_search(opened: Opened, query: str, k: int, depth: int, budget_ms: int | None = None, strict: bool = False, mode: str = "interpolate") -> list[StageRun]:
    """The fused call (depth 0), then — unless `depth` is 0 — the re-ranked call."""
    fused = timed_search(
        opened.handle,
        "fused",
        query,
        xtriever.SearchOptions(k=k, rerank_depth=0, max_time_ms=budget_ms, strict=strict, explain=True),
    )
    if depth == 0:
        return [fused]
    reranked = timed_search(
        opened.handle,
        "re-ranked",
        query,
        xtriever.SearchOptions(
            k=k,
            rerank_depth=depth,
            rerank_mode=rerank_mode_option(mode),
            max_time_ms=budget_ms,
            strict=strict,
            explain=True,
        ),
    )
    return [fused, reranked]


def mode_label(info: xtriever.IndexInfo, mode: str) -> str:
    """`interpolate α 0.5` from the index's recorded mode, or `replace`."""
    if mode == "replace":
        return "replace"
    recorded = info.rerank_mode
    if recorded.is_INTERPOLATE():
        return f"interpolate α {recorded.alpha:g}"
    return "replace"
