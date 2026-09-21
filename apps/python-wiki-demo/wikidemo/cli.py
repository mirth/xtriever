"""The ``wikidemo`` command line (contracts/cli.md): ``search``, ``about``, ``build``,
``measure``. Exit 0 on success (an empty result is success), 1 on a missing or unusable input, an engine
error, a refused build or a parity FAIL, 2 on a usage error.
"""

from __future__ import annotations

import argparse
import sys

import xtriever

from . import DEFAULT_DEPTH, DEFAULT_K, DEPTHS
from .chunking import CHUNKERS, DEFAULT_CHUNKER
from .hits import displayed, marks
from .inputs import MissingInput, UnusableInput, require, resolve
from .render import WARMUP_LINE, dropped_line, empty_line, error_line, list_block, open_line, stage_line, wall_line
from .search import open_artefact, requested_mode_label, run_search

DEPTH_HELP = (
    f"re-rank depth (default {DEFAULT_DEPTH}; the engine's own default is 20). Feature 014 measured depth 10 at "
    "−0.3 mean nDCG@10 on the BEIR sets for half the cross-encoder calls; Feature 018 made it the demos' default"
)


def _common(p: argparse.ArgumentParser) -> None:
    p.add_argument("--artefact", help="the Wikipedia artefact directory (index/, ATTRIBUTION.txt); env XTRIEVER_WIKI_ARTEFACT; default target/xt-wiki")
    p.add_argument("--embedder", help="the embedder directory; env XTRIEVER_MODEL_DIR; default reference/models/all-MiniLM-L6-v2")
    p.add_argument("--reranker", help="the re-ranker directory; env XTRIEVER_RERANK_MODEL_DIR; default reference/models/ms-marco-MiniLM-L-6-v2")


def _positive(text: str) -> int:
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return value


def _non_negative(text: str) -> int:
    value = int(text)
    if value < 0:
        raise argparse.ArgumentTypeError("must not be negative")
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="wikidemo", description="Xtriever's Python Wikipedia demo: search, about, build, measure.")
    sub = parser.add_subparsers(dest="command", required=True)

    s = sub.add_parser("search", help="search the index: the fused list, then the re-ranked list with what moved")
    _common(s)
    s.add_argument("-k", type=_positive, default=DEFAULT_K, help=f"hits to return (default {DEFAULT_K})")
    s.add_argument("--depth", type=int, choices=DEPTHS, default=DEFAULT_DEPTH, help=DEPTH_HELP)
    s.add_argument("--budget-ms", type=_non_negative, default=None, help="time budget in ms; a stage that runs out degrades (or errors under --strict)")
    s.add_argument("--strict", action="store_true", help="raise the engine's error instead of degrading")
    s.add_argument("--mode", choices=("interpolate", "replace"), default="interpolate", help="re-rank order: the engine's interpolating default (α 0.5) or the previous replace order")
    s.add_argument("--explain", action="store_true", help="print the eight pipeline features under each hit")
    s.add_argument("--snippet", type=_positive, default=None, help="cut passages to this many characters")
    s.add_argument("query")

    a = sub.add_parser("about", help="the corpus, the models, the index and the attribution")
    _common(a)

    b = sub.add_parser("build", help="build an index from the raw snapshot through the package alone: verify, exclude, split (the 008 contract chunker, or chonky with --chunker chonky), add, commit, merge")
    _common(b)
    b.add_argument("--out", required=True, help="output directory (must not exist; written as <out>.partial until complete)")
    b.add_argument("--limit", type=_positive, default=None, help="the first N articles only; without it the whole corpus (hours)")
    b.add_argument("--snapshot", help="the snapshot JSONL; default reference/datasets/wiki/simple.jsonl")
    b.add_argument("--manifest", help="the snapshot manifest; default reference/datasets/wiki-manifest.json")
    b.add_argument(
        "--chunker",
        choices=CHUNKERS,
        default=DEFAULT_CHUNKER,
        help="how articles are cut into passages: contract — the Feature 008 contract chunker, the shipped index's recipe (default); "
        "chonky — the chonky neural splitter (needs the chonky extra and its model)",
    )
    b.add_argument("--chonky", help="the chonky splitter model directory (used with --chunker chonky); env XTRIEVER_CHONKY_MODEL_DIR; default reference/models/chonky_distilbert_base_uncased_1")

    m = sub.add_parser("measure", help="run the 20 measurement queries at depths 0/5/10/20, check parity, write a record")
    _common(m)
    group = m.add_mutually_exclusive_group()
    group.add_argument("--expected", help="the host goldens; default <artefact>/expected.json")
    group.add_argument("--against", help="a second artefact to compare live responses with (slice parity)")
    m.add_argument("--queries", help="the measurement queries; default reference/fixtures/008/queries.json")
    m.add_argument("--out", help="the record path; default specs/019-python-wiki-demo/runs/<machine>-<stamp>-mmap-threads<n>.json")
    return parser


def needs_for(args) -> list[str]:
    """The inputs a command must find before anything loads (contracts/cli.md)."""
    if args.command == "build":
        needs = ["embedder", "reranker", "snapshot", "manifest"]
        return needs + ["chonky"] if args.chunker == "chonky" else needs
    if args.command == "measure":
        needs = ["artefact", "embedder", "reranker", "queries"]
        return needs if args.against else needs + ["expected"]
    return ["artefact", "embedder", "reranker"]


def cmd_search(args, paths) -> int:
    opened = open_artefact(paths)
    print(open_line(opened))
    print(WARMUP_LINE)
    # `run_search` yields the fused stage before it starts the re-ranked call, so the fused
    # block is on screen while the cross-encoder works (spec FR-005, as the iOS demo).
    stages = run_search(opened, args.query, k=args.k, depth=args.depth, budget_ms=args.budget_ms, strict=args.strict, mode=args.mode)
    fused = next(stages)
    if not fused.response.hits:
        print()
        print(empty_line(args.query))
        print(stage_line(fused.response.stages, fused.response.elapsed_ms))
        print(wall_line(fused.wall_ms, None, fused.peak_bytes))
        return 0
    print()
    print("\n".join(list_block("fused (lexical + dense)", displayed(fused.response.hits), fused.wall_ms, args.snippet, args.explain)), flush=True)
    reranked_ms = None
    last = fused
    for reranked in stages:
        last = reranked
        reranked_ms = reranked.wall_ms
        m, dropped = marks([h.external_id for h in fused.response.hits], [h.external_id for h in reranked.response.hits])
        print()
        label = f"re-ranked ({requested_mode_label(args.mode)}, depth {args.depth})"
        print("\n".join(list_block(label, displayed(reranked.response.hits, m), reranked.wall_ms, args.snippet, args.explain)))
        line = dropped_line(dropped)
        if line:
            print(line)
    print()
    print(stage_line(last.response.stages, last.response.elapsed_ms))
    print(wall_line(fused.wall_ms, reranked_ms, last.peak_bytes))
    return 0


def cmd_build(args, paths) -> int:
    from .build import run_build

    return run_build(args, paths)


def cmd_about(args, paths) -> int:
    from .about import run_about

    return run_about(args, paths)


def cmd_measure(args, paths) -> int:
    from .measure import run_measure

    return run_measure(args, paths)


COMMANDS = {"search": cmd_search, "about": cmd_about, "build": cmd_build, "measure": cmd_measure}


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        paths = resolve(args)
        require(paths, needs_for(args))
        return COMMANDS[args.command](args, paths)
    except MissingInput as e:
        print(f"wikidemo: {e}\n  produce it with: {e.producer}", file=sys.stderr)
        return 1
    except UnusableInput as e:
        print(f"wikidemo: {e}", file=sys.stderr)
        return 1
    except xtriever.XtrieverError as e:
        print(error_line(e), file=sys.stderr)
        return 1
