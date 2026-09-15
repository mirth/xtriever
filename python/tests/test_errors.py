"""Every engine error kind arrives as its own exception class with the engine's message
(spec FR-003, SC-003). The model-free case is the one refusal reachable without a model:
`open`/`create` load the embedder first."""

import json
import shutil

import pytest

import xtriever
from conftest import EMBEDDER, FIXTURE_INDEX


def test_missing_embedder_is_a_model_error(tmp_path):
    with pytest.raises(xtriever.XtrieverError.Model) as info:
        xtriever.IndexHandle.open(str(tmp_path), str(tmp_path / "no-model"), None, xtriever.LoadPath.MMAP)
    assert "config.json" in str(info.value)


def test_the_base_class_catches_every_kind(tmp_path):
    with pytest.raises(xtriever.XtrieverError):
        xtriever.IndexHandle.open(str(tmp_path), str(tmp_path / "no-model"), None, xtriever.LoadPath.MMAP)


@pytest.mark.models
def test_missing_index_is_corrupt(tmp_path):
    with pytest.raises(xtriever.XtrieverError.Corrupt) as info:
        xtriever.IndexHandle.open(str(tmp_path / "absent"), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    assert "xtriever-pipeline.json" in str(info.value)


@pytest.mark.models
def test_wrong_format_version_is_corrupt(tmp_path):
    bad = tmp_path / "idx"
    shutil.copytree(FIXTURE_INDEX, bad)
    descriptor = bad / "xtriever-pipeline.json"
    d = json.loads(descriptor.read_text())
    d["format_version"] = 99
    descriptor.write_text(json.dumps(d))
    with pytest.raises(xtriever.XtrieverError.Corrupt) as info:
        xtriever.IndexHandle.open(str(bad), str(EMBEDDER), None, xtriever.LoadPath.MMAP)
    assert "format version" in str(info.value)


@pytest.mark.models
def test_strict_budget_is_budget_exhausted(handle):
    with pytest.raises(xtriever.XtrieverError.BudgetExhausted) as info:
        handle.search("lantern", xtriever.SearchOptions(k=5, max_time_ms=1, strict=True))
    assert "ms" in str(info.value)


@pytest.mark.models
def test_create_in_a_non_empty_directory_is_corrupt(tmp_path):
    (tmp_path / "something").write_text("x")
    config = xtriever.IndexConfig(
        fields=[xtriever.FieldDef(name="text", kind=xtriever.FieldKind.TEXT(analyzer="standard"))],
        dense_fields=["text"],
    )
    with pytest.raises(xtriever.XtrieverError.Corrupt):
        xtriever.IndexHandle.create(str(tmp_path), config, str(EMBEDDER), None, xtriever.LoadPath.MMAP)


@pytest.mark.models
def test_create_with_a_bad_dense_field_is_schema(tmp_path):
    config = xtriever.IndexConfig(
        fields=[xtriever.FieldDef(name="text", kind=xtriever.FieldKind.TEXT(analyzer="standard"))],
        dense_fields=["missing"],
    )
    with pytest.raises(xtriever.XtrieverError.Schema):
        xtriever.IndexHandle.create(str(tmp_path / "idx"), config, str(EMBEDDER), None, xtriever.LoadPath.MMAP)
