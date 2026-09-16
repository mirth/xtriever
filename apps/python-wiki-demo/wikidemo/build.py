"""``wikidemo build``: the raw Simple English Wikipedia snapshot to a searchable index with
the Feature 008 recipe, through the package alone (research D7–D10; spec FR-010–FR-013):

    verify the snapshot against its manifest → read the articles → drop the ones the
    manifest's rules exclude → cut each into passages that fit the embedder's window →
    add them (the engine embeds) → commit → merge → write the sidecars → rename into place.

The result has the shipped artefact's layout — `<out>/index/` with `corpus.json` inside,
`<out>/ATTRIBUTION.txt`, `<out>/wiki-build.json` — so `search`, `about` and `measure` work on
it unchanged, and with the same first N articles it is the index the Rust build produces.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import sys
import time
from pathlib import Path

import xtriever

from .chunking import BuildError, Pricer, documents_for
from .hits import wikipedia_url
from .inputs import Paths
from .record import CHUNKER, attribution_text, corpus_identity, dir_bytes, now_rfc3339, threads, write_json
from .rules import excluded_by, parse_rules, rule_name

#: Passages per `add` call — the Rust build's cache shard / ingest batch.
BATCH = 4_096
#: The full corpus's cost, from the 008 record (93 ms per passage at four threads).
FULL_BUILD_WARNING = (
    "full build: 427,947 passages ≈ 11 h on a laptop (Feature 008 measured 93 ms per passage); "
    "Ctrl-C now if that is not what you want"
)
FULL_BUILD_PAUSE_S = 5


def wiki_config() -> xtriever.IndexConfig:
    """The shipped index's schema: `title` (boost 2.0) and `text` (1.0), both `standard_en`;
    `text` is the dense field; the engine's default depths and re-rank mode (D7)."""
    text = xtriever.FieldKind.TEXT(analyzer="standard_en")
    return xtriever.IndexConfig(
        fields=[
            xtriever.FieldDef(name="title", kind=text, indexed=True, stored=False, boost=2.0),
            xtriever.FieldDef(name="text", kind=text, indexed=True, stored=False, boost=1.0),
        ],
        dense_fields=["text"],
    )


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def verify_snapshot(snapshot: Path, manifest: dict) -> None:
    """Bytes and sha256 against the manifest's `jsonl` entry before a line is read."""
    want_bytes, want_sha = int(manifest["jsonl"]["bytes"]), manifest["jsonl"]["sha256"]
    have_bytes = snapshot.stat().st_size
    have_sha = sha256_of(snapshot)
    if have_bytes != want_bytes or have_sha != want_sha:
        raise BuildError(
            f"snapshot {snapshot} does not match the manifest\n"
            f"  expected {want_bytes} bytes sha256={want_sha}\n"
            f"  actual   {have_bytes} bytes sha256={have_sha}\n"
            "  fetch it with: scripts/fetch-wiki.sh"
        )


def _ms(t0: float) -> int:
    return round((time.perf_counter() - t0) * 1000)


def run_build(args, paths: Paths) -> int:
    out = Path(args.out)
    if not out.is_absolute():
        out = Path.cwd() / out
    try:
        build(paths, out, args.limit)
    except BuildError as e:
        print(f"wikidemo: {e}", file=sys.stderr)
        return 1
    return 0


def build(paths: Paths, out: Path, limit: int | None) -> Path:
    if out.exists():
        raise BuildError(f"{out} already exists; choose another --out (nothing is overwritten)")
    manifest = json.loads(paths.manifest.read_text(encoding="utf-8"))
    rules = parse_rules(manifest["exclusions"])
    if limit is None:
        print(FULL_BUILD_WARNING, file=sys.stderr)
        time.sleep(FULL_BUILD_PAUSE_S)

    t_total = time.perf_counter()
    phases: dict[str, int] = {}

    t = time.perf_counter()
    verify_snapshot(paths.snapshot, manifest)
    phases["fetch_verify"] = _ms(t)

    staging = out.with_name(out.name + ".partial")
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    index_dir = staging / "index"

    handle = xtriever.IndexHandle.create(str(index_dir), wiki_config(), str(paths.embedder), str(paths.reranker), xtriever.LoadPath.MMAP)
    info = handle.info()
    pricer = Pricer(paths.embedder)

    counts = {
        "articles": 0,
        "excluded": {rule_name(r): 0 for r in rules},
        "selected": 0,
        "passages": 0,
        "passages_over_window": 0,
        "url_mismatches": 0,
    }
    read_exclude_ms = chunk_ms = embed_ingest_ms = 0
    batch: list[xtriever.Document] = []
    batch_no = 0

    def ingest():
        nonlocal embed_ingest_ms, batch_no, batch
        if not batch:
            return
        t = time.perf_counter()
        handle.add(batch)
        embed_ingest_ms += _ms(t)
        batch_no += 1
        print(
            f"batch {batch_no}: articles {counts['articles']} (excluded {sum(counts['excluded'].values())}), "
            f"passages {counts['passages']}, {round(time.perf_counter() - t_total)} s",
            file=sys.stderr,
        )
        batch = []

    with paths.snapshot.open("r", encoding="utf-8") as fh:
        for line in fh:
            if limit is not None and counts["articles"] >= limit:
                break
            t = time.perf_counter()
            counts["articles"] += 1
            article = json.loads(line)
            rule = excluded_by(rules, article["title"], article["text"])
            if rule is not None:
                counts["excluded"][rule_name(rules[rule])] += 1
                read_exclude_ms += _ms(t)
                continue
            if wikipedia_url(article["title"]) != article["url"]:
                counts["url_mismatches"] += 1
                raise BuildError(f"article {article['id']} ({article['title']!r}): the snapshot's url {article['url']!r} is not the derived {wikipedia_url(article['title'])!r}")
            counts["selected"] += 1
            read_exclude_ms += _ms(t)
            t = time.perf_counter()
            docs = documents_for(article, pricer)
            chunk_ms += _ms(t)
            counts["passages"] += len(docs)
            batch.extend(docs)
            if len(batch) >= BATCH:
                ingest()
    ingest()
    phases["read_exclude"] = read_exclude_ms
    phases["chunk"] = chunk_ms
    phases["embed_ingest"] = embed_ingest_ms

    t = time.perf_counter()
    handle.commit()
    phases["commit"] = _ms(t)
    t = time.perf_counter()
    handle.merge()
    phases["merge"] = _ms(t)
    del handle  # release the writer lock before the rename

    snapshot = {
        "edition": manifest["edition"],
        "snapshot_date": manifest["snapshot_date"],
        "parquet_sha256": manifest["parquet"]["sha256"],
        "jsonl_sha256": manifest["jsonl"]["sha256"],
    }
    identity = corpus_identity(snapshot, manifest["exclusions"], CHUNKER, info.embedder_fingerprint, partial=limit)
    sidecar = {
        "schema_version": 1,
        "corpus_identity": identity,
        "snapshot": snapshot,
        "exclusions": manifest["exclusions"],
        "chunker": CHUNKER,
        "embedder_fingerprint": info.embedder_fingerprint,
    }
    if limit is not None:
        sidecar["partial"] = limit
    sidecar["counts"] = counts
    write_json(index_dir / "corpus.json", sidecar)

    recorded_at = now_rfc3339()
    (staging / "ATTRIBUTION.txt").write_text(attribution_text(manifest, identity, recorded_at), encoding="utf-8")

    phases["total"] = _ms(t_total)
    n_threads, _source = threads()
    artefact_bytes = {str(p.relative_to(index_dir)): p.stat().st_size for p in sorted(index_dir.rglob("*")) if p.is_file()}
    artefact_bytes["total"] = dir_bytes(index_dir)
    record = {
        "schema_version": 1,
        "feature": "019-python-wiki-demo",
        "recorded_at": recorded_at,
        "corpus_identity": identity,
        "partial": limit,
        "host": {
            "os": "macos" if sys.platform == "darwin" else platform.system().lower(),
            "arch": platform.machine(),
            "threads": n_threads,
            "python": sys.version.split()[0],
            "xtriever": xtriever.__version__,
        },
        "models": {"embedder": info.embedder_fingerprint, "reranker_for_demo": info.reranker_model_id},
        "counts": counts,
        "phases_ms": phases,
        "artefact_bytes": artefact_bytes,
    }
    write_json(staging / "wiki-build.json", record)

    os.rename(staging, out)

    print(f"articles: {counts['articles']:,} read, {counts['selected']:,} selected, {counts['passages']:,} passages")
    for name, n in counts["excluded"].items():
        print(f"excluded {name}: {n:,}")
    print("phases: " + " · ".join(f"{k} {v:,} ms" for k, v in phases.items()))
    print(f"corpus identity: {identity}" + (f" (partial: first {limit:,} articles)" if limit is not None else ""))
    print(f"wrote {out}")
    return out
