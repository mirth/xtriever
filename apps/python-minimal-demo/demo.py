"""The smallest Xtriever demo: ten documents, one index, one search — the fused stage
(lexical + dense), then the cross-encoder's re-ordering.

    python apps/python-minimal-demo/demo.py "how do bees make honey"

Needs a Python with the `xtriever` wheel (cd python && .venv/bin/maturin build --release) and
the two pinned models (scripts/fetch-model.sh, once with --manifest
reference/models/manifest-rerank.json); XTRIEVER_MODEL_DIR / XTRIEVER_RERANK_MODEL_DIR override.
Left out on purpose: chunking and a real corpus (apps/python-wiki-demo), explanations, marks
and the stage report (`wikidemo search --explain`), the phone (apps/ios-wiki-demo).
"""
import os, shutil, sys, tempfile
from pathlib import Path
import xtriever

# the corpus: an id and a text per document — a title line, a blank line, the body
DOCS = [
    ("doc-01", "Honey bees\n\nWorker bees collect nectar from flowers and store it in wax combs, where it thickens into honey as water evaporates."),
    ("doc-02", "The water cycle\n\nWater evaporates from seas and lakes, condenses into clouds and falls again as rain or snow."),
    ("doc-03", "Bread\n\nBread is made from flour, water and yeast; the dough rises as the yeast releases gas, then it is baked."),
    ("doc-04", "Tides\n\nThe sea rises and falls twice a day because the Moon's gravity pulls on the oceans."),
    ("doc-05", "The Moon\n\nThe Moon orbits the Earth about once a month and shows phases as sunlight falls on it from different angles."),
    ("doc-06", "Rust\n\nRust forms when iron reacts with oxygen and water; painting or galvanising the metal keeps them apart."),
    ("doc-07", "Sourdough\n\nSourdough bread rises with wild yeast and bacteria kept alive in a starter instead of packaged yeast."),
    ("doc-08", "Coral reefs\n\nCoral reefs are built by tiny animals whose skeletons pile up over centuries in warm, shallow seas."),
    ("doc-09", "Thunderstorms\n\nThunderstorms form when warm, moist air rises quickly; lightning heats the air so fast that it cracks as thunder."),
    ("doc-10", "Glaciers\n\nGlaciers are rivers of ice that flow slowly downhill, carving valleys as they move."),
]

ROOT = Path(__file__).resolve().parents[2]
EMBEDDER = os.environ.get("XTRIEVER_MODEL_DIR", str(ROOT / "reference/models/all-MiniLM-L6-v2"))
RERANKER = os.environ.get("XTRIEVER_RERANK_MODEL_DIR", str(ROOT / "reference/models/ms-marco-MiniLM-L-6-v2"))

# the schema: one text field, also the dense field (Feature 013's layout)
SCHEMA = xtriever.IndexConfig(
    fields=[xtriever.FieldDef(name="contents", kind=xtriever.FieldKind.TEXT(analyzer="standard_en"))],
    dense_fields=["contents"],
)

def build(index_dir):
    # build: create → add → commit → merge (the engine embeds; merge compacts both stages)
    handle = xtriever.IndexHandle.create(index_dir, SCHEMA, EMBEDDER, RERANKER, xtriever.LoadPath.MMAP)
    handle.add([xtriever.Document(external_id=i, fields={"contents": xtriever.FieldValue.TEXT(t)}) for i, t in DOCS])
    handle.commit()
    handle.merge()
    return handle

def run(query):
    # search: the fused stage (depth 0), then the re-ranked stage (the first 10 fused candidates)
    index_dir = tempfile.mkdtemp(prefix="xtriever-minimal-")  # removed below, whatever happens
    handle = None
    try:
        handle = build(index_dir)
        fused = handle.search(query, xtriever.SearchOptions(k=5, rerank_depth=0))
        reranked = handle.search(query, xtriever.SearchOptions(k=5, rerank_depth=10))
    finally:
        del handle  # close the index before removing it
        shutil.rmtree(index_dir)
    return fused, reranked

def print_hits(label, hits):
    # print: rank, id, the engine's fused score, the cross-encoder's score when re-ranked, the title line
    print(f"{label}, {len(hits)} hits")
    for rank, hit in enumerate(hits, 1):
        rerank = "" if hit.rerank_score is None else f"  rerank={hit.rerank_score:.4f}"
        print(f" {rank}. {hit.external_id}  score={hit.score:.4f}{rerank}  {hit.text.splitlines()[0]}")

def main(argv):
    if len(argv) != 1:
        print('usage: demo.py "your question"', file=sys.stderr)
        return 2
    fused, reranked = run(argv[0])
    print(f"indexed {len(DOCS)} documents\n")
    print_hits("fused (lexical + dense)", fused.hits)
    print()
    print_hits("re-ranked (depth 10)", reranked.hits)
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
