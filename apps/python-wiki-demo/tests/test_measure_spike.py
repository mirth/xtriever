"""Feature 028 (the accelerated inference spike): ``measure`` names the compute path it was built
for and digests every response, so two runs can be proven identical (data-model "Run-record
additions"). Spike code — removed or promoted by the follow-up the verdict names.
"""

import hashlib
from types import SimpleNamespace

from wikidemo.cli import build_parser
from wikidemo.measure import hits_digest, make_record

# The shared test vector: the Swift harnesses hold the same one and the same digest.
VECTOR = [
    ("q01", 0, [("a", 0.03, None), ("b", 0.02, None)]),
    ("q01", 10, [("b", 0.02, 8.6), ("a", 0.03, 5.7)]),
    ("q02", 0, [("c", 0.025, None)]),
    ("q02", 10, [("c", 0.025, -1.5)]),
]
CANONICAL = (
    "q01\t0\t1\ta\t3f9eb851eb851eb8\t-\n"
    "q01\t0\t2\tb\t3f947ae147ae147b\t-\n"
    "q01\t10\t1\tb\t3f947ae147ae147b\t4109999a\n"
    "q01\t10\t2\ta\t3f9eb851eb851eb8\t40b66666\n"
    "q02\t0\t1\tc\t3f9999999999999a\t-\n"
    "q02\t10\t1\tc\t3f9999999999999a\tbfc00000\n"
)
DIGEST = "ef38d0b89356527c392fea5745fcc5e3f789feb76926d9c8c0122b257754d509"


def _responses():
    responses = {}
    for qid, depth, hits in VECTOR:
        responses.setdefault(qid, {})[depth] = [
            SimpleNamespace(external_id=i, score=s, rerank_score=r) for i, s, r in hits
        ]
    return responses


def test_the_pinned_digest_is_the_canonical_texts():
    assert hashlib.sha256(CANONICAL.encode("utf-8")).hexdigest() == DIGEST


def test_hits_digest_matches_the_shared_vector():
    assert hits_digest(_responses(), ["q01", "q02"], [0, 10]) == DIGEST


def test_hits_digest_changes_with_any_bit():
    responses = _responses()
    responses["q02"][10][0].rerank_score = -1.5000001
    assert hits_digest(responses, ["q01", "q02"], [0, 10]) != DIGEST


def test_measure_takes_the_labels_with_cpu_defaults():
    parser = build_parser()
    default = parser.parse_args(["measure"])
    assert default.compute_path == "cpu" and default.rerank_batch is False
    labelled = parser.parse_args(["measure", "--compute-path", "metal", "--rerank-batch"])
    assert labelled.compute_path == "metal" and labelled.rerank_batch is True


def test_the_record_carries_the_labels_and_the_digest():
    rec = make_record(
        corpus="wikipedia", index_meta={}, open_ms=0, embedder_load_ms=0, reranker_load_ms=0, warmup_ms=0,
        runs=[], depths=(0, 10), comparison=SimpleNamespace(as_record=lambda: {}), peak_bytes=0,
        compute_path="accelerate", rerank_batch=True, hits_digest=DIGEST,
    )
    assert (rec["computePath"], rec["rerankBatch"], rec["hitsDigest"]) == ("accelerate", True, DIGEST)
