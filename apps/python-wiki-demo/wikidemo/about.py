"""``wikidemo about``: the corpus (from the 008 sidecar), the index and the models (from the
engine's ``info()``), this session's timings, and the attribution verbatim (spec FR-015).
"""

from __future__ import annotations

import json

from . import DEFAULT_DEPTH
from .inputs import Paths
from .render import open_line
from .search import Opened, mode_label, open_artefact

NO_SIDECAR = "(no corpus sidecar)"
LICENCE_URL = "https://creativecommons.org/licenses/by-sa/4.0/"


def read_sidecar(paths: Paths) -> dict | None:
    if not paths.corpus_json.exists():
        return None
    return json.loads(paths.corpus_json.read_text(encoding="utf-8"))


def read_attribution(paths: Paths) -> str | None:
    if not paths.attribution.exists():
        return None
    return paths.attribution.read_text(encoding="utf-8")


def licence_url(paths: Paths) -> str:
    """The manifest's licence URL when the manifest is on disk; else the CC BY-SA 4.0 URL."""
    if paths.manifest.exists():
        try:
            return json.loads(paths.manifest.read_text(encoding="utf-8"))["licence"]["url"]
        except (OSError, ValueError, KeyError):
            pass
    return LICENCE_URL


def about_lines(opened: Opened, sidecar: dict | None, attribution: str | None, licence: str) -> list[str]:
    info = opened.info
    lines = [open_line(opened), ""]
    if sidecar is None:
        lines += [
            f"corpus: {NO_SIDECAR}",
            f"articles: {NO_SIDECAR}",
            f"corpus identity: {NO_SIDECAR}",
        ]
    else:
        snap, counts = sidecar["snapshot"], sidecar["counts"]
        lines += [
            f"corpus: Simple English Wikipedia ({snap['edition']}), snapshot {snap['snapshot_date']}",
            f"articles: {counts['articles']:,} read, {counts['selected']:,} selected, {counts['passages']:,} passages",
            f"corpus identity: {sidecar['corpus_identity']}",
        ]
        if "partial" in sidecar and sidecar["partial"] is not None:
            lines.append(f"partial: first {sidecar['partial']:,} articles")
    lines += [
        f"passages (documents): {info.documents:,}",
        f"format version: {info.format_version}",
        f"embedder: {info.embedder_fingerprint}",
        f"re-ranker: {info.reranker_model_id}",
        f"candidate depth: {info.candidate_depth}",
        f"rrf k: {info.rrf_k}",
        f"re-rank depth (engine default): {info.rerank_depth}",
        f"re-rank depth (demo default): {DEFAULT_DEPTH}",
        f"re-rank mode: {mode_label(info, 'interpolate')}",
        f"open: {opened.open_ms} ms",
        f"embedder load: {info.embedder_load_ms} ms",
        f"re-ranker load: {info.reranker_load_ms} ms",
        "",
    ]
    lines.append(attribution.rstrip("\n") if attribution is not None else "(no ATTRIBUTION.txt beside the index)")
    lines.append(f"licence: {licence}")
    return lines


def run_about(args, paths: Paths) -> int:
    opened = open_artefact(paths)
    print("\n".join(about_lines(opened, read_sidecar(paths), read_attribution(paths), licence_url(paths))))
    return 0
