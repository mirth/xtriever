#!/usr/bin/env python3
"""Feature 027 goldens: the sparse document encoder's expansions and the query-side terms.

Principle II says a golden comes from `reference/`, not from the implementation it checks. This
generator encodes a fixed set of documents with the pinned encoder
(`reference/models/manifest-sparse-doc-v3.json`) through `transformers`' own
`DistilBertForMaskedLM` — CPU, float32, eager attention, one document per call — with the recipe
the Feature 012 spike measured (`reference/sparse_spike.py::encode_documents`, research D3):

    tokenise (truncated to 512 tokens, special tokens included)
    → the masked-LM logits' maximum over positions, per vocabulary entry
    → log1p(relu(·)) → log1p(·) → special tokens zeroed → entries > 0, ascending id

and each query's terms with the query-side rule (research D7): the distinct token ids of the
query (no truncation, no added special tokens) that are not special tokens and have a positive
entry in `idf.json`, ascending. Queries involve no model.

It writes `reference/fixtures/027/`:

* `documents.json` — each document's text, its token ids after truncation, its untruncated
  length and whether it was truncated;
* `expansions.json` — each document's kept `(token id, weight)` entries, weights as float32;
* `queries.json` — each query's text and its kept token ids;
* `manifest.json` — every file's SHA-256, this generator's, the model's revision and the
  tolerances the Rust tests apply (research D9).

    reference/.venv-027/bin/python reference/gen_027_fixtures.py          # write the fixtures
    reference/.venv-027/bin/python reference/gen_027_fixtures.py --check  # re-derive; exit 1 on any difference

Environment: `scripts/setup-reference-venv.sh 027`; the model: `scripts/fetch-model.sh --manifest
reference/models/manifest-sparse-doc-v3.json`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "reference/fixtures/027"
MODEL_MANIFEST = ROOT / "reference/models/manifest-sparse-doc-v3.json"
MAX_TOKENS = 512

# The tolerances the Rust oracle applies (research D9), recorded beside the goldens they govern.
TOLERANCE = {
    "weight_abs": 1e-4,
    "scale": 10,
    "half_margin": 1e-3,
}

_LONG = " ".join(
    f"Section {i}. The retrieval engine indexes documents in an inverted index and scores them "
    f"with BM25; a dense encoder maps each passage to a vector, and the two result lists are fused "
    f"by reciprocal rank before a cross-encoder re-ranks the head of the list. Section {i} adds "
    f"that vocabulary mismatch between a question and its answer is what expansion addresses."
    for i in range(1, 13)
)

# Authored here so the fixture does not depend on a dataset: short and long, technical and
# plain, non-ASCII, a document of a few words and an empty one (research D9).
DOCUMENTS = [
    ("d01", "Vitamin D deficiency is associated with an increased risk of respiratory infections in children."),
    ("d02", "How do I roll over a 401(k) into an IRA without paying taxes or penalties on the transfer?"),
    ("d03", "The mitochondrion is the powerhouse of the cell: it produces ATP through oxidative phosphorylation."),
    ("d04", "Stock buybacks reduce the number of shares outstanding, which raises earnings per share."),
    ("d05", "Café Müller in São Paulo serves naïve résumé writers crème brûlée; 東京 and Москва are cities too."),
    ("d06", "Hello."),
    ("d07", ""),
    ("d08", _LONG),
    ("d09", "BRCA1 mutations increase lifetime breast cancer risk; prophylactic mastectomy lowers it substantially."),
    ("d10", "Compound interest: the interest earned on interest, compounding monthly versus annually."),
    ("d11", "Tokenization splits text into wordpieces such as token, ##ization and ##s before encoding."),
    ("d12", "A [SEP] token and a [CLS] token appear literally in this text, alongside ordinary words."),
]

# Queries: a question, keywords, repeated words, non-ASCII, literal special tokens, an empty one.
QUERIES = [
    ("q1", "does vitamin d prevent colds"),
    ("q2", "rollover 401k to ira taxes"),
    ("q3", "cell cell cell energy energy"),
    ("q4", "what raises earnings per share"),
    ("q5", "cafés in São Paulo and 東京"),
    ("q6", "[CLS] mitochondria [SEP]"),
    ("q7", ""),
    ("q8", "Is compounding monthly better than compounding annually for a savings account with a high rate?"),
]


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def model_dir() -> Path:
    manifest = json.loads(MODEL_MANIFEST.read_text(encoding="utf-8"))
    return ROOT / "reference/models" / manifest["local_dir"]


def derive() -> dict[str, dict]:
    """Every fixture file's content, computed from the pinned model."""
    import numpy as np
    import torch
    import transformers
    from transformers import AutoTokenizer, DistilBertForMaskedLM

    torch.manual_seed(0)
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)

    manifest = json.loads(MODEL_MANIFEST.read_text(encoding="utf-8"))
    directory = model_dir()
    for f in manifest["files"]:
        got = sha256_file(directory / f["name"])
        if got != f["sha256"]:
            sys.exit(f"gen_027: {directory / f['name']} has sha256 {got}, expected {f['sha256']}")

    tokenizer = AutoTokenizer.from_pretrained(directory)
    model = DistilBertForMaskedLM.from_pretrained(directory, dtype=torch.float32, attn_implementation="eager")
    model.eval()
    special = sorted({tokenizer.convert_tokens_to_ids(t) for t in tokenizer.special_tokens_map.values()})

    idf_by_token = json.loads((directory / "idf.json").read_text(encoding="utf-8"))
    idf = np.zeros(len(tokenizer), dtype=np.float64)
    for token, weight in idf_by_token.items():
        idf[tokenizer.convert_tokens_to_ids(token)] = weight

    documents, expansions = [], {}
    with torch.inference_mode():
        for doc_id, text in DOCUMENTS:
            full = tokenizer(text, truncation=False, add_special_tokens=True)["input_ids"]
            feature = tokenizer(text, truncation=True, max_length=MAX_TOKENS, return_tensors="pt", return_token_type_ids=False)
            logits = model(**feature).logits[0]  # [tokens, vocab]; one document, no padding
            values = torch.max(logits, dim=0).values
            values = torch.log1p(torch.log1p(torch.relu(values)))
            values[special] = 0
            row = values.float().numpy()
            kept = np.nonzero(row > 0)[0]
            ids = feature["input_ids"][0].tolist()
            documents.append(
                {
                    "id": doc_id,
                    "text": text,
                    "input_ids": ids,
                    "untruncated_tokens": len(full),
                    "truncated": len(full) > MAX_TOKENS,
                }
            )
            expansions[doc_id] = [[int(i), float(row[i])] for i in kept]

    queries = []
    for query_id, text in QUERIES:
        ids = tokenizer(text, truncation=False, add_special_tokens=False)["input_ids"]
        terms = sorted({i for i in ids if i not in special and idf[i] > 0})
        queries.append({"id": query_id, "text": text, "terms": terms})

    return {
        "documents.json": {"max_tokens": MAX_TOKENS, "special_ids": special, "documents": documents},
        "expansions.json": {"activation": manifest["activation"], "expansions": expansions},
        "queries.json": {"queries": queries},
        "_provenance": {
            "model": f"{manifest['repository']}@{manifest['revision']}",
            "torch": torch.__version__,
            "transformers": transformers.__version__,
            "numpy": np.__version__,
        },
    }


def render(document: dict) -> str:
    return json.dumps(document, indent=1, ensure_ascii=False, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="re-derive every fixture; exit 1 on any difference")
    args = parser.parse_args()

    derived = derive()
    provenance = derived.pop("_provenance")
    files = {name: render(content) for name, content in derived.items()}
    manifest = {
        "generator": "reference/gen_027_fixtures.py",
        "generator_sha256": sha256_file(Path(__file__).resolve()),
        "files": {name: hashlib.sha256(text.encode("utf-8")).hexdigest() for name, text in files.items()},
        "tolerance": TOLERANCE,
        **provenance,
    }
    files["manifest.json"] = render(manifest)

    if args.check:
        differ = [name for name, text in files.items() if not (OUT / name).is_file() or (OUT / name).read_text(encoding="utf-8") != text]
        for name in differ:
            print(f"gen_027: {OUT.relative_to(ROOT) / name} differs from the reference", file=sys.stderr)
        if differ:
            return 1
        print(f"gen_027: PASS — {len(files)} files in {OUT.relative_to(ROOT)} match the reference")
        return 0

    OUT.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        (OUT / name).write_text(text, encoding="utf-8")
    for doc_id, entries in derived["expansions.json"]["expansions"].items():
        print(f"  {doc_id}: {len(entries)} entries")
    print(f"gen_027: wrote {len(files)} files to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
