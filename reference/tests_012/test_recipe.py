"""The document and query recipes equal the model card's (research D1): the card's own example
(`What's the weather in ny now?` vs `Currently New York is rainy.`) reproduces its printed
weights and similarity; queries need no model call. Red until the encoder exists."""

import json

import pytest

from conftest import REPO

MANIFEST = REPO / "reference" / "models" / "manifest-sparse-doc-v3.json"

# The card prints these (query weight, document weight) pairs for the example, 4 decimals.
CARD_EXAMPLE = {
    "ny": (5.7729, 0.8049),
    "weather": (4.5684, 0.9710),
    "now": (3.5895, 0.4720),
    "?": (3.3313, 0.0286),
    "what": (2.7699, 0.0787),
    "in": (0.4989, 0.0417),
}
CARD_SIMILARITY = 11.1105


@pytest.fixture(scope="module")
def loaded():
    from sparse_spike import load_model

    if not MANIFEST.exists():
        pytest.skip("manifest not pinned")
    return load_model(MANIFEST, device="cpu")


def test_document_recipe_matches_the_card(loaded):
    from sparse_spike import encode_documents, encode_queries, id_to_token

    m = loaded
    doc = encode_documents(m, ["Currently New York is rainy."], batch=1)
    q = encode_queries(m, ["What's the weather in ny now?"])
    names = id_to_token(m)
    doc_w = {names[i]: w for i, w in zip(doc.indices[0], doc.data[0])}
    q_w = {names[i]: w for i, w in zip(q.indices[0], q.data[0])}
    for token, (qw, dw) in CARD_EXAMPLE.items():
        assert abs(q_w[token] - qw) < 1e-3, (token, q_w.get(token), qw)
        assert abs(doc_w[token] - dw) < 1e-3, (token, doc_w.get(token), dw)
    sim = sum(q_w[t] * doc_w.get(t, 0.0) for t in q_w)
    assert abs(sim - CARD_SIMILARITY) < 1e-2, sim
    assert all(w > 0 for w in doc.data[0]) and all(w > 0 for w in q.data[0])
    assert "[CLS]" not in doc_w and "[SEP]" not in doc_w and "[CLS]" not in q_w


def test_long_documents_truncate_and_encode(loaded):
    from sparse_spike import encode_documents

    text = " ".join(["retrieval"] * 1000)
    out = encode_documents(loaded, [text], batch=1)
    assert out.truncated == 1
    assert len(out.indices[0]) > 0


def test_queries_never_call_the_model(loaded, monkeypatch):
    import torch

    from sparse_spike import encode_queries

    def boom(*a, **k):  # pragma: no cover — must not run
        raise AssertionError("the query side called the model")

    monkeypatch.setattr(torch.nn.Module, "__call__", boom)
    q = encode_queries(loaded, ["what is xtriever"])
    idf = json.loads((REPO / "reference" / "models" / loaded.local_dir / "idf.json").read_text())
    from sparse_spike import id_to_token

    names = id_to_token(loaded)
    # Exactly the query's distinct non-special token ids with a non-zero IDF, ascending.
    tok = loaded.tokenizer
    ids = tok("what is xtriever", add_special_tokens=True)["input_ids"]
    expected = sorted({i for i in ids if i not in set(loaded.special_ids) and idf.get(names[i], 0) > 0})
    assert expected, "the example query must tokenise to something"
    assert q.indices[0].tolist() == expected
    for i, w in zip(q.indices[0], q.data[0]):
        assert abs(w - idf[names[i]]) < 1e-6, names[i]
