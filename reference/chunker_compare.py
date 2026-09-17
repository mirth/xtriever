#!/usr/bin/env python3
"""Side by side: the same articles chunked two ways, searched with the same queries.

A spike after Feature 021 (specs/021-chonky-wiki-chunking/chunker-comparison.md): the
2,000-article slice built by the Rust CLI with the 008 contract chunker versus the same
slice built by `wikidemo build` with the chonky splitter. For each of the 20 measurement
queries and each depth, the two heads are compared by *article* (passage ids differ by
construction), and chonky's head passages are checked against the embedder's 256-position
window. Numbers only; the judgement is the reader's.

    apps/python-wiki-demo/.venv/bin/python reference/chunker_compare.py \
        --contract target/xt-wiki-slice-rs --chonky target/xt-wiki-slice-chonky
"""

import argparse
import json
import statistics
from pathlib import Path

import xtriever
from tokenizers import Tokenizer

REPO = Path(__file__).resolve().parents[1]
EMBEDDER = REPO / "reference/models/all-MiniLM-L6-v2"
RERANKER = REPO / "reference/models/ms-marco-MiniLM-L-6-v2"
QUERIES = REPO / "reference/fixtures/008/queries.json"
K = 10
DEPTHS = (0, 10)
WINDOW = 256


def open_index(artefact: Path):
    return xtriever.IndexHandle.open(str(artefact / "index"), str(EMBEDDER), str(RERANKER), xtriever.LoadPath.MMAP)


def title_of(hit):
    return hit.text.split("\n\n", 1)[0]


def snippet(hit, n=90):
    body = hit.text.split("\n\n", 1)[1] if "\n\n" in hit.text else hit.text
    body = " ".join(body.split())
    return body[:n] + ("…" if len(body) > n else "")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--contract", required=True, type=Path)
    ap.add_argument("--chonky", required=True, type=Path)
    ap.add_argument("--out", type=Path, default=None, help="write the per-query listing here (markdown)")
    args = ap.parse_args()

    tok = Tokenizer.from_file(str(EMBEDDER / "tokenizer.json"))
    tok.no_truncation()
    tok.no_padding()
    positions = lambda s: len(tok.encode(s, add_special_tokens=True).ids)  # noqa: E731

    a, b = open_index(args.contract), open_index(args.chonky)
    queries = json.loads(QUERIES.read_text(encoding="utf-8"))
    lines = []
    agg = {d: {"jaccard": [], "top1_same": 0, "over_in_head": 0, "head_positions_contract": [], "head_positions_chonky": []} for d in DEPTHS}

    for q in queries:
        lines.append(f"\n### {q['id']} — {q['text']}\n")
        for depth in DEPTHS:
            opts = xtriever.SearchOptions(k=K, rerank_depth=depth)
            ra, rb = a.search(q["text"], opts), b.search(q["text"], opts)
            arts_a = [h.chunk.parent for h in ra.hits]
            arts_b = [h.chunk.parent for h in rb.hits]
            sa, sb = set(arts_a), set(arts_b)
            jaccard = len(sa & sb) / len(sa | sb) if sa | sb else 1.0
            agg[depth]["jaccard"].append(jaccard)
            agg[depth]["top1_same"] += bool(arts_a and arts_b and arts_a[0] == arts_b[0])
            pos_b = [positions(h.text) for h in rb.hits]
            pos_a = [positions(h.text) for h in ra.hits]
            over = sum(p > WINDOW for p in pos_b)
            agg[depth]["over_in_head"] += over
            agg[depth]["head_positions_contract"].extend(pos_a)
            agg[depth]["head_positions_chonky"].extend(pos_b)
            lines.append(f"**depth {depth}** — articles in both heads (Jaccard) {jaccard:.2f}; top-1 article {'same' if arts_a[:1] == arts_b[:1] else 'differs'}; chonky head passages over the window: {over}/{len(rb.hits)}\n")
            lines.append("| # | contract chunker | chonky |")
            lines.append("|---|---|---|")
            for i in range(5):
                ca = f"{title_of(ra.hits[i])} `{ra.hits[i].external_id}` ({pos_a[i]}) — {snippet(ra.hits[i])}" if i < len(ra.hits) else ""
                cb = f"{title_of(rb.hits[i])} `{rb.hits[i].external_id}` ({pos_b[i]}{'*' if pos_b[i] > WINDOW else ''}) — {snippet(rb.hits[i])}" if i < len(rb.hits) else ""
                lines.append(f"| {i + 1} | {ca} | {cb} |")
            lines.append("")

    summary = ["## Summary\n", "| depth | mean Jaccard of head articles | top-1 article same | chonky head passages over the window | head passage positions, contract (median / max) | head passage positions, chonky (median / max) |", "|---|---|---|---|---|---|"]
    for d in DEPTHS:
        g = agg[d]
        summary.append(
            f"| {d} | {statistics.mean(g['jaccard']):.2f} | {g['top1_same']}/{len(queries)} | {g['over_in_head']}/{len(queries) * K} "
            f"| {statistics.median(g['head_positions_contract']):.0f} / {max(g['head_positions_contract'])} "
            f"| {statistics.median(g['head_positions_chonky']):.0f} / {max(g['head_positions_chonky'])} |"
        )
    text = "\n".join(summary) + "\n\n## Per query (top 5; `*` = over the 256-position window)\n" + "\n".join(lines) + "\n"
    print("\n".join(summary))
    if args.out:
        args.out.write_text(text, encoding="utf-8")
        print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
