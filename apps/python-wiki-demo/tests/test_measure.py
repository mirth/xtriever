"""The measurement's comparison is the device test's rule (research D13; spec FR-016):
lexical bits exact, fused order identical at depth 0, dense / re-rank scores within 1e-3 per
document matched by id, every golden query, depth and hit present — and the record's shape
is the contract's, with no hostname in it."""

import platform
import struct
from types import SimpleNamespace

import pytest
import xtriever

from conftest import EMBEDDER, RERANKER, f32_bits, f64_bits
from wikidemo.measure import (
    QueryRun,
    TruthHit,
    compare,
    load_truth,
    make_record,
    summarise,
    truth_from_hits,
)


def _f32(bits):
    return None if bits is None else struct.unpack("<f", struct.pack("<I", int(bits, 16)))[0]


def _f64(bits):
    return None if bits is None else struct.unpack("<d", struct.pack("<Q", int(bits, 16)))[0]


def _truth(external_id, score=0.03, bm25=4.5, dense=0.25, rerank=None, rerank_rank=None, combined=None):
    return TruthHit(
        external_id=external_id,
        score_bits=f64_bits(score),
        bm25_score_bits=f32_bits(bm25),
        dense_score_bits=f32_bits(dense),
        rerank_score_bits=f32_bits(rerank),
        rerank_rank=rerank_rank,
        rerank_combined_bits=None if combined is None else f64_bits(combined),
    )


def _hit_from(t, bm25_delta=0.0, dense_delta=0.0, rerank_delta=0.0):
    """A response hit that reproduces a truth hit, optionally perturbed."""
    bm25 = _f32(t.bm25_score_bits)
    dense = _f32(t.dense_score_bits)
    rerank = _f32(t.rerank_score_bits)
    return SimpleNamespace(
        external_id=t.external_id,
        score=_f64(t.score_bits),
        rerank_score=None if rerank is None else rerank + rerank_delta,
        explain=SimpleNamespace(
            bm25_score=None if bm25 is None else bm25 + bm25_delta,
            bm25_rank=None,
            dense_score=None if dense is None else dense + dense_delta,
            dense_rank=None,
            fused=_f64(t.score_bits),
            rerank_score=None if rerank is None else rerank + rerank_delta,
            rerank_rank=t.rerank_rank,
            rerank_combined=_f64(t.rerank_combined_bits),
        ),
    )


DEPTHS = (0, 5, 10, 20)


def _world():
    """Two queries × four depths of truth, and the matching responses."""
    truth, responses = {}, {}
    for qid in ("q01", "q02"):
        truth[qid], responses[qid] = {}, {}
        for d in DEPTHS:
            hits = [
                _truth(f"{qid}-a", score=0.03, rerank=None if d == 0 else 8.6, rerank_rank=None if d == 0 else 1, combined=None if d == 0 else 1.0),
                _truth(f"{qid}-b", score=0.02, bm25=None, rerank=None if d == 0 else 5.7, rerank_rank=None if d == 0 else 2, combined=None if d == 0 else 0.4),
            ]
            truth[qid][d] = hits
            responses[qid][d] = [_hit_from(t) for t in hits]
    return truth, responses


def test_pass_when_everything_matches():
    truth, responses = _world()
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "PASS"
    assert c.queries_compared == 2 and c.lexical_bit_identical == 2 and c.fused_order_identical == 2
    assert c.dense_max_abs_diff == 0.0 and c.rerank_max_abs_diff == 0.0
    assert c.hits_compared == 16 and c.all_bits_identical == 16
    assert c.incomplete == [] and c.tolerance_abs == 1e-3


def test_incomplete_on_missing_query():
    truth, responses = _world()
    del responses["q02"]
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "FAIL" and c.queries_compared == 1 and "q02: not searched" in c.incomplete


def test_incomplete_on_depth_set_mismatch():
    truth, responses = _world()
    del truth["q01"][10]
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "FAIL"
    assert any(m.startswith("q01: goldens carry depths [0, 5, 20]") for m in c.incomplete)


def test_incomplete_on_hit_count_and_unknown_id():
    truth, responses = _world()
    responses["q01"][5] = responses["q01"][5][:1]
    c = compare(truth, responses, DEPTHS)
    assert "q01@5: 1 hits, host has 2" in c.incomplete and c.verdict == "FAIL"
    truth, responses = _world()
    responses["q02"][0][1].external_id = "stranger"
    c = compare(truth, responses, DEPTHS)
    assert "q02@0: stranger is not among the host's hits" in c.incomplete


def test_incomplete_on_score_present_one_side_only():
    truth, responses = _world()
    responses["q01"][20][0].rerank_score = None
    c = compare(truth, responses, DEPTHS)
    assert "q01@20 q01-a: re-rank score present on one side only" in c.incomplete
    truth, responses = _world()
    responses["q01"][0][0].explain.dense_score = None
    c = compare(truth, responses, DEPTHS)
    assert "q01@0 q01-a: dense score present on one side only" in c.incomplete


def test_lexical_bits_must_be_exact():
    truth, responses = _world()
    responses["q01"][10][0].explain.bm25_score += 1e-6
    c = compare(truth, responses, DEPTHS)
    assert c.lexical_bit_identical == 1 and c.verdict == "FAIL"
    assert c.all_bits_identical == 15


def test_fused_order_only_at_depth_zero():
    truth, responses = _world()
    responses["q01"][10].reverse()
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "PASS" and c.fused_order_identical == 2
    truth, responses = _world()
    responses["q01"][0].reverse()
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "FAIL" and c.fused_order_identical == 1


def test_tolerance_is_1e_3():
    truth, responses = _world()
    responses["q01"][5][0].explain.dense_score += 9e-4
    responses["q02"][20][1].rerank_score += 9e-4
    c = compare(truth, responses, DEPTHS)
    assert c.verdict == "PASS" and 8e-4 < c.dense_max_abs_diff < 1e-3 and 8e-4 < c.rerank_max_abs_diff < 1e-3
    assert c.all_bits_identical == 14
    truth, responses = _world()
    responses["q01"][5][0].explain.dense_score += 2e-3
    assert compare(truth, responses, DEPTHS).verdict == "FAIL"


def test_truth_from_hits_round_trips():
    truth, responses = _world()
    assert truth_from_hits(responses["q01"][20]) == truth["q01"][20]


def test_load_truth_reads_the_goldens_shape(tmp_path):
    golden = {
        "generated_by": "x",
        "info": {},
        "queries": [
            {"id": "q01", "text": "why", "depths": {"0": {"hits": [{"external_id": "a", "score_bits": f64_bits(0.03), "bm25_score_bits": f32_bits(4.5), "dense_score_bits": None, "rerank_score_bits": None, "rerank_rank": None, "rerank_combined_bits": None}]}}}
        ],
    }
    import json

    p = tmp_path / "expected.json"
    p.write_text(json.dumps(golden), encoding="utf-8")
    truth, texts = load_truth(p)
    assert texts == {"q01": "why"}
    assert truth == {"q01": {0: [_truth("a", score=0.03, bm25=4.5, dense=None)]}}


def test_medians_and_latency_keys():
    runs = []
    for i, qid in enumerate(("q01", "q02", "q03")):
        for d, ms in zip(DEPTHS, (100 + i, 400 + i, 800 + i, 1500 + i)):
            runs.append(QueryRun(id=qid, depth=d, elapsed_ms=ms, engine_ms=ms - 1, hits=10, peak_bytes_after=1000))
    s = summarise(runs, DEPTHS)
    assert s["perDepthMedianMs"] == {"0": 101, "5": 401, "10": 801, "20": 1501}
    assert s["perDepthMaxMs"] == {"0": 102, "5": 402, "10": 802, "20": 1502}
    assert s["latency"] == {
        "medianFusedMs": 101,
        "maxFusedMs": 102,
        "medianRerankedMs": 801,
        "maxRerankedMs": 802,
        "medianRerankedAtEngineDefaultMs": 1501,
        "medianTotalMs": 902,
        "maxTotalMs": 904,
    }


def test_record_shape_and_no_hostname():
    truth, responses = _world()
    c = compare(truth, responses, DEPTHS)
    runs = [QueryRun(id="q01", depth=d, elapsed_ms=10, engine_ms=9, hits=2, peak_bytes_after=5) for d in DEPTHS]
    rec = make_record(
        corpus="wikipedia",
        index_meta={"bytes": 1, "documents": 2, "formatVersion": 2, "embedderFingerprint": "e", "rerankerModelId": "r", "corpusIdentity": None, "partial": None},
        open_ms=1,
        embedder_load_ms=2,
        reranker_load_ms=3,
        warmup_ms=4,
        runs=runs,
        depths=DEPTHS,
        comparison=c,
        peak_bytes=5,
    )
    assert set(rec) == {
        "schemaVersion", "feature", "corpus", "machine", "os", "python", "xtrieverVersion", "build", "index",
        "openMs", "embedderLoadMs", "rerankerLoadMs", "warmupMs", "queries", "perDepthMedianMs", "perDepthMaxMs",
        "latency", "footprint", "parity", "notes", "recordedAt",
    }
    assert rec["schemaVersion"] == 1 and rec["feature"] == "019-python-wiki-demo"
    assert rec["parity"] == {
        "queriesCompared": 2, "lexicalBitIdentical": 2, "fusedOrderIdentical": 2, "denseMaxAbsDiff": 0.0,
        "rerankMaxAbsDiff": 0.0, "allBitsIdentical": 16, "hitsCompared": 16, "toleranceAbs": 0.001, "verdict": "PASS",
    }
    assert rec["footprint"] == {"peakBytes": 5, "peakMethod": "ru_maxrss", "ceilingBytes": 600_000_000, "underCeiling": True}
    assert rec["queries"][0] == {"id": "q01", "depth": 0, "elapsedMs": 10, "engineMs": 9, "hits": 2, "peakBytesAfter": 5}
    assert rec["build"]["loadPath"] == "mmap" and rec["build"]["effectiveThreads"] >= 1
    assert rec["xtrieverVersion"] == xtriever.__version__
    node = platform.node()
    if node:
        import json

        assert node not in json.dumps(rec) and node.split(".")[0] not in json.dumps(rec)
    against = make_record(corpus="wikipedia-slice", index_meta={}, open_ms=0, embedder_load_ms=0, reranker_load_ms=0, warmup_ms=0, runs=runs, depths=DEPTHS, comparison=c, peak_bytes=1, against={"artefact": "x"})
    assert against["against"] == {"artefact": "x"}


@pytest.mark.models
def test_fixture_goldens_pass(fixture_goldens, fixture_handle):
    truth, responses = {}, {}
    depths = set()
    for q in fixture_goldens["queries"]:
        d = q["rerank_depth"]
        depths.add(d)
        truth[q["id"]] = {d: [TruthHit(**{k: h[k] for k in TruthHit.__dataclass_fields__}) for h in q["with_reranker"]["hits"]]}
        r = fixture_handle.search(q["text"], xtriever.SearchOptions(k=q["k"], rerank_depth=d, explain=True))
        responses[q["id"]] = {d: r.hits}
    assert len(depths) == 1
    c = compare(truth, responses, tuple(sorted(depths)))
    assert c.verdict == "PASS", c.incomplete
    assert c.all_bits_identical == c.hits_compared
