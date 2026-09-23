"""The text layout of contracts/cli.md (research D16). Pure functions from the engine's values
(and the wall clocks) to lines; nothing here computes a score.
"""

from __future__ import annotations

from .hits import DisplayedHit, render_feature
from .record import megabytes

WARMUP_LINE = "warm-up: the first search of a process pages the vectors in"


def open_line(opened) -> str:
    info = opened.info
    reranker = "none" if info.reranker_load_ms is None else f"{info.reranker_load_ms} ms"
    return (
        f"opened {opened.paths.artefact} ({info.documents:,} passages, format {info.format_version})"
        f" · embedder {info.embedder_load_ms} ms · re-ranker {reranker} · open {opened.open_ms} ms · mmap"
    )


def list_block(label: str, hits: list[DisplayedHit], wall_ms: int, snippet: int | None = None, explain: bool = False) -> list[str]:
    header = f"{label}, {len(hits)} hits, {wall_ms} ms"
    if snippet is not None:
        header += f", passages cut to {snippet} characters"
    lines = [header]
    with_marks = any(h.mark is not None for h in hits)
    for h in hits:
        head = f"{h.rank:2d}. "
        if with_marks:
            head += f"{(h.mark.render() if h.mark else ''):<4} "
        head += f"{h.title}  {h.external_id}"
        if h.ordinal is not None:
            head += f"  passage {h.ordinal + 1}"
        lines.append(head)
        if h.url is not None:
            lines.append(f"    {h.url}")
        passage = h.passage
        if snippet is not None and len(passage) > snippet:
            passage = passage[:snippet] + "…"
        lines.extend(f"    {line}" for line in passage.split("\n"))
        if explain:
            for name, value in h.features:
                lines.append(f"    {name}: {render_feature(name, value)}")
    return lines


def dropped_line(dropped: list[tuple[str, int]]) -> str | None:
    if not dropped:
        return None
    return "dropped from the head: " + ", ".join(f"{eid} (was {rank})" for eid, rank in dropped)


def reason_label(reason) -> str:
    """A `DegradeReason` in words."""
    if reason.is_BUDGET_EXCEEDED():
        return f"budget exceeded: {reason.elapsed_ms} ms of {reason.limit_ms}"
    if reason.is_STAGE_ERROR():
        return f"stage error: {reason.message}"
    return str(reason)


def stage_line(stages, elapsed_ms: int) -> str:
    parts = [f"lexical {stages.lexical_candidates}"]
    parts.append("dense skipped" if stages.dense_candidates is None else f"dense {stages.dense_candidates}")
    rr = stages.rerank
    if rr is None:
        parts.append("re-rank none")
    else:
        text = f"re-rank {rr.candidates} candidates, {rr.scored} scored"
        if rr.skipped is not None:
            text += f", skipped ({reason_label(rr.skipped)})"
        parts.append(text)
    if stages.degraded is not None:
        parts.append(f"degraded: {stages.degraded.stage} ({reason_label(stages.degraded.reason)})")
    parts.append(f"time limit ignored: {'yes' if stages.time_limit_ignored else 'no'}")
    parts.append(f"engine {elapsed_ms} ms")
    return "stages: " + " · ".join(parts)


def wall_line(fused_ms: int, reranked_ms: int | None, peak_bytes: int) -> str:
    parts = [f"fused {fused_ms} ms"]
    if reranked_ms is not None:
        parts.append(f"re-ranked {reranked_ms} ms")
        parts.append(f"total {fused_ms + reranked_ms:,} ms")
    parts.append(f"peak resident {megabytes(peak_bytes)}")
    return "wall: " + " · ".join(parts)


def empty_line(query: str) -> str:
    return f'no passages found for "{query}"'


def error_line(exc: Exception) -> str:
    """`wikidemo: <Kind>: <the engine's message>` — the error's other fields, if any, in brackets."""
    fields = {k: v for k, v in vars(exc).items() if not k.startswith("_")}
    message = fields.pop("message", None)
    text = str(message) if message is not None else str(exc)
    if fields:
        text += " [" + ", ".join(f"{k}={v}" for k, v in sorted(fields.items())) + "]"
    return f"wikidemo: {type(exc).__name__}: {text}"
