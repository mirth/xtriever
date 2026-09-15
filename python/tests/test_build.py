"""An index built from Python is the engine's index (spec US4, SC-007): the 005 fixture's
documents through the wire types, committed, searched — equal to the 007 goldens minted
from the Rust-built fixture index. Red until PR B lands the builder exports."""

import json

import pytest

import xtriever
from conftest import EMBEDDER, FIXTURE_DOCS, RERANKER, golden_hit_tuples, search_hit_tuples

pytestmark = pytest.mark.models


def fixture():
    return json.loads(FIXTURE_DOCS.read_text())


def field_kind(kind):
    if isinstance(kind, dict) and "Text" in kind:
        return xtriever.FieldKind.TEXT(analyzer=kind["Text"])
    return {
        "Keyword": xtriever.FieldKind.KEYWORD,
        "U64": xtriever.FieldKind.U64,
        "I64": xtriever.FieldKind.I64,
        "F64": xtriever.FieldKind.F64,
        "Bool": xtriever.FieldKind.BOOL,
        "DateMillis": xtriever.FieldKind.DATE_MILLIS,
    }[kind]()


def field_value(value):
    (kind, v), = value.items()
    return {
        "Text": xtriever.FieldValue.TEXT,
        "Keyword": xtriever.FieldValue.KEYWORD,
        "U64": xtriever.FieldValue.U64,
        "I64": xtriever.FieldValue.I64,
        "F64": xtriever.FieldValue.F64,
        "Bool": xtriever.FieldValue.BOOL,
        "DateMillis": xtriever.FieldValue.DATE_MILLIS,
    }[kind](v)


def config(h):
    return xtriever.IndexConfig(
        fields=[
            xtriever.FieldDef(
                name=f["name"], kind=field_kind(f["kind"]), indexed=f["indexed"], stored=f["stored"], boost=f["boost"]
            )
            for f in h["schema"]["fields"]
        ],
        dense_fields=list(h["dense_fields"]),
    )


def document(d):
    chunk = d.get("chunk")
    if chunk is not None:
        rng = chunk.get("byte_range")
        chunk = xtriever.ChunkInfo(
            parent=chunk["parent"],
            ordinal=chunk["ordinal"],
            byte_start=None if rng is None else rng[0],
            byte_end=None if rng is None else rng[1],
        )
    return xtriever.Document(
        external_id=d["external_id"], fields={k: field_value(v) for k, v in d["fields"].items()}, chunk=chunk
    )


def build(tmp_path, h, reranker=True):
    handle = xtriever.IndexHandle.create(
        str(tmp_path / "idx"), config(h), str(EMBEDDER), str(RERANKER) if reranker else None, xtriever.LoadPath.MMAP
    )
    handle.add([document(d) for d in h["documents"]])
    handle.commit()
    return handle


def test_python_built_index_equals_the_goldens(tmp_path, goldens):
    h = fixture()
    handle = build(tmp_path, h)
    fused = xtriever.IndexHandle.open(str(tmp_path / "idx"), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    assert handle.info().documents == 40
    assert handle.contains("d001") and not handle.contains("nope")
    pairs = 0
    for q in goldens["queries"]:
        for hd, key in ((fused, "without_reranker"), (handle, "with_reranker")):
            r = hd.search(q["text"], xtriever.SearchOptions(k=q["k"], rerank_depth=q["rerank_depth"], explain=True))
            assert search_hit_tuples(r.hits) == golden_hit_tuples(q[key]["hits"]), (q["id"], key)
            pairs += 1
    assert pairs == 16


def test_staged_changes_are_invisible_until_commit(tmp_path):
    h = fixture()
    handle = build(tmp_path, h)
    d001 = next(d for d in h["documents"] if d["external_id"] == "d001")
    replaced = dict(d001, fields=dict(d001["fields"], text={"Text": "zebraquark zebraquark zebraquark"}))
    handle.add([document(replaced)])
    assert handle.contains("d001")
    before = handle.search("zebraquark", xtriever.SearchOptions(k=5, rerank_depth=0))
    assert all(hit.external_id != "d001" or "zebraquark" not in hit.text for hit in before.hits)
    handle.commit()
    after = handle.search("zebraquark", xtriever.SearchOptions(k=5, rerank_depth=0))
    assert after.hits and after.hits[0].external_id == "d001" and "zebraquark" in after.hits[0].text

    handle.delete(["d001", "unknown-id"])
    assert handle.contains("d001"), "a delete is staged, not committed"
    handle.commit()
    assert not handle.contains("d001")
    assert handle.info().documents == 39
    reopened = xtriever.IndexHandle.open(str(tmp_path / "idx"), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    assert not reopened.contains("d001") and reopened.info().documents == 39


def test_add_embedded_refusals(tmp_path):
    h = fixture()
    handle = xtriever.IndexHandle.create(str(tmp_path / "idx"), config(h), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    doc = document(h["documents"][0])
    with pytest.raises(xtriever.XtrieverError.DimensionMismatch):
        handle.add_embedded([doc], [[0.1, 0.2, 0.3]])
    with pytest.raises(xtriever.XtrieverError.Schema):
        handle.add_embedded([doc, document(h["documents"][1])], [[0.0] * 384])


def test_create_refusals(tmp_path):
    h = fixture()
    (tmp_path / "busy").mkdir()
    (tmp_path / "busy" / "something").write_text("x")
    with pytest.raises(xtriever.XtrieverError.Corrupt):
        xtriever.IndexHandle.create(str(tmp_path / "busy"), config(h), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    bad = config(h)
    bad.dense_fields = ["not-a-field"]
    with pytest.raises(xtriever.XtrieverError.Schema):
        xtriever.IndexHandle.create(str(tmp_path / "idx2"), bad, str(EMBEDDER), None, xtriever.LoadPath.MMAP)
