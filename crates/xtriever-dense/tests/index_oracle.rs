//! The version-1 oracle (Feature 024, spec FR-010; research D8): scripted add / replace /
//! delete / commit / reopen sequences with queries after every commit, whose results were
//! minted **on the version-1 implementation** (`mint`, ignored) into
//! `tests/support/v1_oracle.json` — every hit's id and score bits. `replay` asserts that the
//! current implementation reproduces them bit for bit, so the append/tombstone/compact format
//! cannot change a single result. The generator is a fixed-seed LCG: deterministic without a
//! dependency, and the file is the record either way.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xtriever_core::{DocId, Metric, VectorIndex};
use xtriever_dense::FlatIndex;

const DIM: usize = 8;
const STEPS: usize = 300;
const QUERIES_PER_COMMIT: usize = 5;
const KS: [usize; 3] = [1, 5, 13];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Step {
    Add {
        id: u32,
        vector: Vec<f32>,
    },
    Replace {
        id: u32,
        vector: Vec<f32>,
    },
    Delete {
        ids: Vec<u32>,
    },
    Commit,
    Reopen,
    /// After a commit: `len()` and the queries' hits as `(id, score bits)`.
    Expect {
        len: u64,
        queries: Vec<Query>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Query {
    vector: Vec<f32>,
    k: usize,
    allowed: Option<Vec<u32>>,
    hits: Vec<(u32, u32)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Sequence {
    metric: String,
    seed: u64,
    steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Oracle {
    schema_version: u32,
    dim: usize,
    fingerprint: String,
    sequences: Vec<Sequence>,
}

/// A 64-bit LCG (Knuth's MMIX constants); enough for reproducible test data.
struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() >> 33) as usize % n
    }
    /// A finite vector with components in roughly [-1, 1], never all-zero.
    fn vector(&mut self) -> Vec<f32> {
        loop {
            let v: Vec<f32> = (0..DIM)
                .map(|_| ((self.next_u64() >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0)
                .collect();
            if v.iter().any(|x| *x != 0.0) {
                return v;
            }
        }
    }
}

fn oracle_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/v1_oracle.json")
}

/// Generate the mutation steps (without expectations) for one sequence.
fn generate_steps(rng: &mut Lcg) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut next_id: u32 = 0;
    let mut known: Vec<u32> = Vec::new(); // ids ever added (may be deleted)
    for _ in 0..STEPS {
        match rng.below(10) {
            0..=4 => {
                steps.push(Step::Add {
                    id: next_id,
                    vector: rng.vector(),
                });
                known.push(next_id);
                next_id += 1;
            }
            5 | 6 if !known.is_empty() => {
                let id = known[rng.below(known.len())];
                steps.push(Step::Replace {
                    id,
                    vector: rng.vector(),
                });
            }
            7 if !known.is_empty() => {
                let n = 1 + rng.below(3);
                let mut ids: Vec<u32> = (0..n).map(|_| known[rng.below(known.len())]).collect();
                if rng.below(4) == 0 {
                    ids.push(next_id + 1000); // an unknown id: a no-op
                }
                steps.push(Step::Delete { ids });
            }
            8 => steps.push(Step::Commit),
            _ => {
                if rng.below(3) == 0 {
                    steps.push(Step::Reopen);
                } else {
                    steps.push(Step::Commit);
                }
            }
        }
    }
    steps.push(Step::Commit);
    steps
}

/// Run the steps on the current implementation; after every `Commit` insert (mint) or check
/// (replay) an `Expect`. Returns the steps with expectations.
fn drive(seq: &Sequence, dim: usize, fingerprint: &str, mint: bool, rng: &mut Lcg) -> Vec<Step> {
    let metric = support::parse_metric(&seq.metric);
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), dim, metric, fingerprint).unwrap();
    let mut out = Vec::new();
    let mut known: Vec<u32> = Vec::new();
    let mut steps = seq.steps.iter().peekable();
    while let Some(step) = steps.next() {
        match step {
            Step::Add { id, vector } | Step::Replace { id, vector } => {
                index.add(DocId(*id), vector).unwrap();
                if !known.contains(id) {
                    known.push(*id);
                }
                out.push(step.clone());
            }
            Step::Delete { ids } => {
                let ids: Vec<DocId> = ids.iter().map(|&i| DocId(i)).collect();
                index.delete(&ids).unwrap();
                out.push(step.clone());
            }
            Step::Reopen => {
                drop(index);
                index = FlatIndex::open(tmp.path()).unwrap();
                out.push(step.clone());
            }
            Step::Commit => {
                index.commit().unwrap();
                out.push(step.clone());
                let expect = if mint {
                    let mut queries = Vec::new();
                    for q in 0..QUERIES_PER_COMMIT {
                        let vector = rng.vector();
                        let k = KS[rng.below(KS.len())];
                        let allowed = if q < 2 && !known.is_empty() {
                            let n = 1 + rng.below(known.len());
                            let mut ids: Vec<u32> =
                                (0..n).map(|_| known[rng.below(known.len())]).collect();
                            ids.sort_unstable();
                            ids.dedup();
                            Some(ids)
                        } else {
                            None
                        };
                        queries.push(Query {
                            vector,
                            k,
                            allowed,
                            hits: Vec::new(),
                        });
                    }
                    Step::Expect { len: 0, queries }
                } else {
                    steps.next().cloned().expect("an Expect after every Commit")
                };
                let Step::Expect { len, queries } = expect else {
                    panic!("expected an Expect step")
                };
                let mut filled = Vec::with_capacity(queries.len());
                for q in queries {
                    let set = q.allowed.as_deref().map(support::doc_set);
                    let hits: Vec<(u32, u32)> = index
                        .search(&q.vector, set.as_ref(), q.k)
                        .unwrap()
                        .into_iter()
                        .map(|h| (h.id.0, h.score.to_bits()))
                        .collect();
                    if !mint {
                        assert_eq!(
                            hits, q.hits,
                            "{} seed {}: hits differ",
                            seq.metric, seq.seed
                        );
                    }
                    filled.push(Query { hits, ..q });
                }
                let got_len = index.len();
                if !mint {
                    assert_eq!(
                        got_len, len,
                        "{} seed {}: len differs",
                        seq.metric, seq.seed
                    );
                }
                out.push(Step::Expect {
                    len: got_len,
                    queries: filled,
                });
            }
            Step::Expect { .. } => panic!("an Expect not preceded by a Commit"),
        }
    }
    out
}

/// Mint `tests/support/v1_oracle.json` from the implementation this test is compiled against.
/// Run once on version 1; the file is the oracle from then on.
#[test]
#[ignore]
fn mint() {
    let fingerprint = "oracle-fingerprint";
    let mut sequences = Vec::new();
    for (i, metric) in [Metric::Cosine, Metric::Dot, Metric::Euclidean]
        .iter()
        .enumerate()
    {
        let seed = 0x5EED_0024 + i as u64;
        let mut rng = Lcg(seed);
        let name = match metric {
            Metric::Cosine => "cosine",
            Metric::Dot => "dot",
            Metric::Euclidean => "euclidean",
        };
        let seq = Sequence {
            metric: name.to_owned(),
            seed,
            steps: generate_steps(&mut rng),
        };
        let steps = drive(&seq, DIM, fingerprint, true, &mut rng);
        sequences.push(Sequence { steps, ..seq });
    }
    let oracle = Oracle {
        schema_version: 1,
        dim: DIM,
        fingerprint: fingerprint.to_owned(),
        sequences,
    };
    std::fs::write(oracle_path(), serde_json::to_vec(&oracle).unwrap()).unwrap();
}

#[test]
fn replay() {
    let oracle: Oracle =
        serde_json::from_slice(&std::fs::read(oracle_path()).expect("mint first")).unwrap();
    assert_eq!(oracle.sequences.len(), 3);
    for seq in &oracle.sequences {
        let mut rng = Lcg(seq.seed); // unused on replay; the file carries everything
        let commits = seq
            .steps
            .iter()
            .filter(|s| matches!(s, Step::Commit))
            .count();
        assert!(commits >= 20, "{}: only {commits} commits", seq.metric);
        let out = drive(seq, oracle.dim, &oracle.fingerprint, false, &mut rng);
        assert_eq!(out, seq.steps, "{}: the replayed steps differ", seq.metric);
    }
}
