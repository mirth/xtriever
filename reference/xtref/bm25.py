"""BM25 and analyzer transcriptions of tantivy 0.26.2, shared by every fixture generator.

This is a *reimplementation from the documented formula and the cited source*, not a call into
tantivy. That is what makes it a reference implementation under Principle II; a Python binding to
the same engine would satisfy the letter of the rule and none of its purpose.

Extracted from ``gen_001_fixtures.py`` for Feature 002 (research D17). Feature 001's fixtures
regenerate byte-identically through this module — that identity is asserted by quickstart Step 2
and is the proof the move changed nothing.
"""

from __future__ import annotations

import math
from bisect import bisect_left

# --------------------------------------------------------------------------------------------
# tantivy 0.26.2 BM25, transcribed. Every constant is cited; none is folklore.
#   src/query/bm25.rs:8-9      K1 = 1.2, B = 0.75
#   src/query/bm25.rs:52-56    idf = ln(1 + (N - df + 0.5) / (df + 0.5))
#   src/query/bm25.rs:168      weight = idf * (1.0 + K1)
#   src/query/bm25.rs:58-59    tf_component = K1 * (1 - B + B * fieldnorm / avg_fieldnorm)
#   src/query/bm25.rs:190-193  tf_factor  = tf / (tf + tf_component)
#   src/query/bm25.rs:111      avg_fieldnorm = total_num_tokens / total_num_docs
#
# The subtlety a naive BM25 gets wrong: only the PER-DOCUMENT length is quantized (to a u8 id via
# FIELD_NORMS_TABLE); the average is computed from raw, unquantized token counts.
# --------------------------------------------------------------------------------------------
K1 = 1.2
B = 0.75

# src/fieldnorm/code.rs:13 -- extracted verbatim, identity for ids 0..=40 then coarsening.
FIELD_NORMS_TABLE: tuple[int, ...] = (
    0, 1, 2, 3, 4, 5, 6, 7,
    8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23,
    24, 25, 26, 27, 28, 29, 30, 31,
    32, 33, 34, 35, 36, 37, 38, 39,
    40, 42, 44, 46, 48, 50, 52, 54,
    56, 60, 64, 68, 72, 76, 80, 84,
    88, 96, 104, 112, 120, 128, 136, 144,
    152, 168, 184, 200, 216, 232, 248, 264,
    280, 312, 344, 376, 408, 440, 472, 504,
    536, 600, 664, 728, 792, 856, 920, 984,
    1048, 1176, 1304, 1432, 1560, 1688, 1816, 1944,
    2072, 2328, 2584, 2840, 3096, 3352, 3608, 3864,
    4120, 4632, 5144, 5656, 6168, 6680, 7192, 7704,
    8216, 9240, 10264, 11288, 12312, 13336, 14360, 15384,
    16408, 18456, 20504, 22552, 24600, 26648, 28696, 30744,
    32792, 36888, 40984, 45080, 49176, 53272, 57368, 61464,
    65560, 73752, 81944, 90136, 98328, 106520, 114712, 122904,
    131096, 147480, 163864, 180248, 196632, 213016, 229400, 245784,
    262168, 294936, 327704, 360472, 393240, 426008, 458776, 491544,
    524312, 589848, 655384, 720920, 786456, 851992, 917528, 983064,
    1048600, 1179672, 1310744, 1441816, 1572888, 1703960, 1835032, 1966104,
    2097176, 2359320, 2621464, 2883608, 3145752, 3407896, 3670040, 3932184,
    4194328, 4718616, 5242904, 5767192, 6291480, 6815768, 7340056, 7864344,
    8388632, 9437208, 10485784, 11534360, 12582936, 13631512, 14680088, 15728664,
    16777240, 18874392, 20971544, 23068696, 25165848, 27263000, 29360152, 31457304,
    33554456, 37748760, 41943064, 46137368, 50331672, 54525976, 58720280, 62914584,
    67108888, 75497496, 83886104, 92274712, 100663320, 109051928, 117440536, 125829144,
    134217752, 150994968, 167772184, 184549400, 201326616, 218103832, 234881048, 251658264,
    268435480, 301989912, 335544344, 369098776, 402653208, 436207640, 469762072, 503316504,
    536870936, 603979800, 671088664, 738197528, 805306392, 872415256, 939524120, 1006632984,
    1073741848, 1207959576, 1342177304, 1476395032, 1610612760, 1744830488, 1879048216, 2013265944,
)
assert len(FIELD_NORMS_TABLE) == 256
assert list(FIELD_NORMS_TABLE) == sorted(FIELD_NORMS_TABLE)


def fieldnorm_to_id(fieldnorm: int) -> int:
    """tantivy src/fieldnorm/code.rs:9 -- ``binary_search(..).unwrap_or_else(|idx| idx - 1)``."""
    idx = bisect_left(FIELD_NORMS_TABLE, fieldnorm)
    if idx < len(FIELD_NORMS_TABLE) and FIELD_NORMS_TABLE[idx] == fieldnorm:
        return idx
    return idx - 1


def id_to_fieldnorm(fieldnorm_id: int) -> int:
    """tantivy src/fieldnorm/code.rs:3."""
    return FIELD_NORMS_TABLE[fieldnorm_id]


def analyze_standard(text: str) -> list[str]:
    """tantivy's ``default`` analyzer, transcribed.

    ``SimpleTokenizer -> RemoveLongFilter::limit(40) -> LowerCaser``
    (src/tokenizer/tokenizer_manager.rs:56-64).

    * SimpleTokenizer splits on ``!char::is_alphanumeric()`` (src/tokenizer/simple_tokenizer.rs:34).
    * RemoveLongFilter keeps a token when ``token.text.len() < 40`` -- that is the **byte** length,
      and it runs **before** LowerCaser (src/tokenizer/remove_long.rs:36).
    """
    tokens: list[str] = []
    current: list[str] = []
    for ch in text:
        if ch.isalnum():
            current.append(ch)
        elif current:
            tokens.append("".join(current))
            current = []
    if current:
        tokens.append("".join(current))
    # RemoveLongFilter (byte length, pre-lowercase), then LowerCaser.
    return [t.lower() for t in tokens if len(t.encode("utf-8")) < 40]


def idf(doc_freq: int, doc_count: int) -> float:
    """tantivy src/query/bm25.rs:52-56."""
    x = ((doc_count - doc_freq) + 0.5) / (doc_freq + 0.5)
    return math.log(1.0 + x)




# Feature 001 called the default chain ``analyze``; keep the name so 001's generator is unchanged.
analyze = analyze_standard


def analyze_standard_en(text: str) -> list[str]:
    """tantivy's ``en_stem`` analyzer, transcribed.

    ``SimpleTokenizer -> RemoveLongFilter::limit(40) -> LowerCaser -> Stemmer::new(Language::English)``
    (src/tokenizer/tokenizer_manager.rs:66-77). The stemmer runs after lowercasing ("The stemmer
    does not lowercase", tokenizer_manager.rs:73). rust-stemmers 1.2.0 (tantivy Cargo.toml:335) is
    generated from the Snowball sources; ``snowballstemmer`` ships the same algorithm, so the two
    are expected to agree (research R1). ``xtriever-core`` names this analyzer ``standard_en``.
    """
    import snowballstemmer  # pinned in requirements-002; imported lazily so 001 needs no stemmer

    stemmer = snowballstemmer.stemmer("english")
    return [stemmer.stemWord(t) for t in analyze_standard(text)]


ANALYZERS = {
    "standard": analyze_standard,
    "standard_en": analyze_standard_en,
}


def bm25_weight(doc_freq: int, doc_count: int) -> float:
    """tantivy src/query/bm25.rs:168 -- ``idf * (1.0 + K1)``."""
    return idf(doc_freq, doc_count) * (1.0 + K1)


def tf_component(fieldnorm_id: int, avg_fieldnorm: float) -> float:
    """tantivy src/query/bm25.rs:58-59 -- per-document length term, from the *quantized* norm."""
    return K1 * (1.0 - B + B * id_to_fieldnorm(fieldnorm_id) / avg_fieldnorm)


def bm25_term_score(tf: int, weight: float, tf_comp: float) -> float:
    """tantivy src/query/bm25.rs:190-193 -- ``weight * tf / (tf + tf_component)``."""
    if tf == 0:
        return 0.0
    return weight * tf / (tf + tf_comp)
