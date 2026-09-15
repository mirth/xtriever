"""The spike's BM25 over an expansion field and its RRF, checked by hand (research D5).
Red until the functions exist."""

import math

import numpy as np
import scipy.sparse as sp


def test_bm25_over_field_by_hand():
    from sparse_spike import bm25_over_field

    # Three documents, three terms, quantised weights as term frequencies.
    tf = sp.csr_matrix(np.array([[3, 0, 1], [0, 2, 0], [1, 1, 1]], dtype=np.float32))
    n = 3
    df = np.array([2, 2, 2])  # t0 in d0,d2; t1 in d1,d2; t2 in d0,d2
    idf = [math.log(1 + (n - d + 0.5) / (d + 0.5)) for d in df]
    lengths = np.array([4, 2, 3])
    avg = lengths.mean()
    k1, b = 1.2, 0.75

    def term(t, d):
        f = tf[d, t]
        return idf[t] * f * (k1 + 1) / (f + k1 * (1 - b + b * lengths[d] / avg))

    want = [term(0, d) + term(2, d) for d in range(3)]  # query tokens {t0, t2}
    got = bm25_over_field(tf, [0, 2], k1=k1, b=b)
    assert np.allclose(got, want, atol=1e-6), (got, want)
    # d0 (tf 3 and 1) beats d2 (1 and 1); d1 has neither term → 0.
    assert got[0] > got[2] > got[1] == 0.0


def test_rrf_by_hand():
    from sparse_spike import rrf

    fused = rrf([["a", "b", "c"], ["c", "a", "d"]], k=60)
    # a: 1/61 + 1/62; c: 1/63 + 1/61; b: 1/62; d: 1/63
    assert [d for d, _ in fused] == ["a", "c", "b", "d"]
    assert abs(fused[0][1] - (1 / 61 + 1 / 62)) < 1e-12
    assert abs(fused[1][1] - (1 / 63 + 1 / 61)) < 1e-12


def test_top_k_ties_break_by_ascending_doc_id():
    from sparse_spike import top_k

    ids = np.array(["d9", "d2", "d5"])
    scores = np.array([1.0, 1.0, 0.0])
    assert top_k(ids, scores, k=10) == ["d2", "d9"]  # zero excluded, tie by id
