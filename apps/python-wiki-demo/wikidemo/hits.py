"""What a hit means on screen — derived from the engine's `Hit` and nothing else.

The title / passage split and the article URL are the Feature 008 corpus convention as the
Swift package (`Hit.titleAndPassage`, `Hit.wikipediaURL`) and the CLI (`wiki::url::derive_url`)
implement it; the change marks are the iOS demo's `ChangeMark.compute`; the eight feature
names are the package's `HitExplain.features()` list (research D3, D4).
"""

from __future__ import annotations

from dataclasses import dataclass

WIKI_BASE = "https://simple.wikipedia.org/wiki/"

FEATURE_NAMES = [
    "bm25.score",
    "bm25.rank",
    "dense.score",
    "dense.rank",
    "fused.score",
    "rerank.score",
    "rerank.rank",
    "rerank.combined",
]

NOT_SEEN = "not seen by this stage"


def title_and_passage(text: str) -> tuple[str, str] | None:
    """The title line and the body: `text` split at its first blank line; None without one."""
    at = text.find("\n\n")
    if at < 0:
        return None
    return text[:at], text[at + 2 :]


def wikipedia_url(title: str) -> str:
    """`https://simple.wikipedia.org/wiki/` + the title with every UTF-8 byte outside
    `A–Z a–z 0–9 - _ . ~ /` percent-encoded (upper-case hex) — exactly the CLI's rule."""
    out = []
    for byte in title.encode("utf-8"):
        c = chr(byte)
        if ("A" <= c <= "Z") or ("a" <= c <= "z") or ("0" <= c <= "9") or c in "-_.~/":
            out.append(c)
        else:
            out.append("%%%02X" % byte)
    return WIKI_BASE + "".join(out)


def features(explain) -> list[tuple[str, float | int | None]]:
    """The eight pipeline features in the engine's names; None where a stage did not see the hit."""
    if explain is None:
        return [(name, None) for name in FEATURE_NAMES]
    return [
        ("bm25.score", explain.bm25_score),
        ("bm25.rank", explain.bm25_rank),
        ("dense.score", explain.dense_score),
        ("dense.rank", explain.dense_rank),
        ("fused.score", explain.fused),
        ("rerank.score", explain.rerank_score),
        ("rerank.rank", explain.rerank_rank),
        ("rerank.combined", explain.rerank_combined),
    ]


def render_feature(name: str, value) -> str:
    if value is None:
        return NOT_SEEN
    if name.endswith(".rank"):
        return str(int(value))
    return f"{value:.4f}"


@dataclass(frozen=True)
class Mark:
    """How a hit's position changed between the fused and the re-ranked list."""

    kind: str  # "new" | "same" | "up" | "down"
    n: int = 0

    def render(self) -> str:
        if self.kind == "up":
            return f"↑{self.n}"
        if self.kind == "down":
            return f"↓{self.n}"
        if self.kind == "same":
            return "="
        return "new"


def marks(fused_ids: list[str], reranked_ids: list[str]) -> tuple[dict[str, Mark], list[tuple[str, int]]]:
    """Marks for every re-ranked hit, and the fused hits (with their 1-based fused rank) that
    fell out of the re-ranked list — the iOS demo's rule, keyed by external id."""
    fused_rank = {eid: rank for rank, eid in enumerate(fused_ids)}
    out: dict[str, Mark] = {}
    seen: set[str] = set()
    for rank, eid in enumerate(reranked_ids):
        seen.add(eid)
        before = fused_rank.get(eid)
        if before is None:
            out[eid] = Mark("new")
            continue
        delta = before - rank
        out[eid] = Mark("same") if delta == 0 else (Mark("up", delta) if delta > 0 else Mark("down", -delta))
    dropped = [(eid, rank + 1) for rank, eid in enumerate(fused_ids) if eid not in seen]
    return out, dropped


@dataclass(frozen=True)
class DisplayedHit:
    rank: int
    external_id: str
    title: str
    passage: str
    url: str | None
    parent: str | None
    ordinal: int | None
    score: float
    rerank_score: float | None
    mark: Mark | None
    features: list[tuple[str, float | int | None]]


def displayed(hits, marks: dict[str, Mark] | None = None) -> list[DisplayedHit]:
    """The engine's hits as the screens show them, ranks 1-based. A text without a title line
    (an index built by someone else) shows the external id as the title and no link."""
    out = []
    for i, hit in enumerate(hits):
        split = title_and_passage(hit.text)
        if split is None:
            title, passage, url = hit.external_id, hit.text, None
        else:
            title, passage = split
            url = wikipedia_url(title)
        chunk = getattr(hit, "chunk", None)
        out.append(
            DisplayedHit(
                rank=i + 1,
                external_id=hit.external_id,
                title=title,
                passage=passage,
                url=url,
                parent=None if chunk is None else chunk.parent,
                ordinal=None if chunk is None else chunk.ordinal,
                score=hit.score,
                rerank_score=hit.rerank_score,
                mark=None if marks is None else marks.get(hit.external_id),
                features=features(getattr(hit, "explain", None)),
            )
        )
    return out
