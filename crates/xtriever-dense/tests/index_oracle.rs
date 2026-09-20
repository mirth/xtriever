//! The dense stage's scripted oracle: add / replace / delete / commit / reopen sequences with
//! queries after every commit, and every hit's id and score bits.
//!
//! **It changed with Feature 026.** Through formats 1 and 2 this file held results minted on the
//! version-1 implementation, and `replay` proved that the append/tombstone/compact format could
//! not change a single bit. Format 3 stores eight-bit codes and *does* change the scores, by
//! design (ADR-0015, spec FR-003), so pretending the old numbers still hold would be a lie.
//! The oracle was re-minted for format 3 into `tests/support/v3_oracle.json`, and what `replay`
//! now proves is narrower but still worth having: that the same scripted sequence gives the same
//! results on every run, through appends, compactions and reopens.
//!
//! The generator is a fixed-seed LCG (`support::Lcg`): deterministic without a dependency. What
//! keeps the file honest is `reference/gen_026_fixtures.py --check-oracle`, which recomputes
//! every expectation in Python from the contract's arithmetic, so the fixture is reproducible
//! independently of this crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use support::Lcg;
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

fn oracle_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/v3_oracle.json")
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
                    vector: rng.vector(DIM),
                });
                known.push(next_id);
                next_id += 1;
            }
            5 | 6 if !known.is_empty() => {
                let id = known[rng.below(known.len())];
                steps.push(Step::Replace {
                    id,
                    vector: rng.vector(DIM),
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
                        let vector = rng.vector(DIM);
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

/// Mint `tests/support/v3_oracle.json` from the implementation this test is compiled against.
/// Run once per format change, and only with the owner's decision recorded — re-minting is how
/// an oracle stops being one, so it belongs to a feature that says why (here, ADR-0015).
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
