//! Feature 024 (spec FR-011, SC-001, SC-004; research D8): the exact scan over the version-2
//! row file against the version-1 shape, and the cost of a 10-row commit against the rewrite
//! version 1 did. 100,000 rows × 384 dims from a fixed-seed generator; `k = 10`.
//!
//! The version-1 *scan shape* is kept here as bench-local code (three columns, the same `f64`
//! accumulation and the same total order) so SC-004's comparison outlives the format's removal;
//! it is a comparison, not an assertion — the product kernel may change. The version-1 write
//! volume needs no measurement: a rewrite is `rows × (8 + 4·dim)` bytes per commit by
//! construction (154 MB at 100k × 384), stated in ADR-0013.
//!
//! ```sh
//! cargo bench -p xtriever-dense --bench scan
//! ```
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

use std::cmp::Ordering;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use xtriever_core::{DocId, DocSet, Metric, VectorIndex};
use xtriever_dense::FlatIndex;

#[path = "../tests/support/mod.rs"]
mod support;
use support::Lcg;

const ROWS: u32 = 100_000;
const DIM: usize = 384;
const K: usize = 10;

fn rows() -> Vec<Vec<f32>> {
    let mut rng = Lcg(0x0024_BE4C);
    (0..ROWS).map(|_| rng.vector(DIM)).collect()
}

fn query() -> Vec<f32> {
    Lcg(0xC0FFEE).vector(DIM)
}

// ── the version-1 shape: three columns, the same arithmetic ────────────────────────────────

struct Columnar {
    ids: Vec<u32>,
    norms: Vec<f32>,
    vectors: Vec<f32>,
}

impl Columnar {
    fn build(rows: &[Vec<f32>]) -> Self {
        let mut ids = Vec::with_capacity(rows.len());
        let mut norms = Vec::with_capacity(rows.len());
        let mut vectors = Vec::with_capacity(rows.len() * DIM);
        for (i, r) in rows.iter().enumerate() {
            ids.push(i as u32);
            norms.push(
                r.iter()
                    .map(|x| f64::from(*x) * f64::from(*x))
                    .sum::<f64>()
                    .sqrt() as f32,
            );
            vectors.extend_from_slice(r);
        }
        Self {
            ids,
            norms,
            vectors,
        }
    }

    /// Cosine scan, `(score DESC, id ASC)`, first `k` — the version-1 loop.
    fn search(&self, q: &[f32], k: usize) -> Vec<(f32, u32)> {
        let q_norm = q
            .iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>()
            .sqrt();
        let mut scored: Vec<(f32, u32)> = Vec::with_capacity(self.ids.len());
        for (i, &id) in self.ids.iter().enumerate() {
            let row = &self.vectors[i * DIM..(i + 1) * DIM];
            let dot: f64 = q
                .iter()
                .zip(row)
                .map(|(&a, &b)| f64::from(a) * f64::from(b))
                .sum();
            let s = (dot / (q_norm * f64::from(self.norms[i]))) as f32;
            scored.push((s, id));
        }
        scored.sort_unstable_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        scored.truncate(k);
        scored
    }
}

fn bench_scan(c: &mut Criterion) {
    let rows = rows();
    let q = query();
    let columnar = Columnar::build(&rows);
    let tmp = tempfile::tempdir().unwrap();
    let mut index = FlatIndex::create(tmp.path(), DIM, Metric::Cosine, "bench").unwrap();
    for (i, r) in rows.iter().enumerate() {
        index.add(DocId(i as u32), r).unwrap();
    }
    index.commit().unwrap();
    let mut allowed = DocSet::new();
    for i in (0..ROWS).step_by(2) {
        allowed.insert(DocId(i));
    }

    let mut g = c.benchmark_group("scan");
    g.sample_size(20);
    g.bench_function(BenchmarkId::new("v1_shape", ROWS), |b| {
        b.iter(|| black_box(columnar.search(black_box(&q), K)))
    });
    g.bench_function(BenchmarkId::new("v2", ROWS), |b| {
        b.iter(|| black_box(index.search(black_box(&q), None, K).unwrap()))
    });
    g.bench_function(BenchmarkId::new("v2_filtered_half", ROWS), |b| {
        b.iter(|| black_box(index.search(black_box(&q), Some(&allowed), K).unwrap()))
    });
    g.finish();
}

fn bench_commit(c: &mut Criterion) {
    let rows = rows();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("v2");
    let mut index = FlatIndex::create(&dir, DIM, Metric::Cosine, "bench").unwrap();
    for (i, r) in rows.iter().enumerate() {
        index.add(DocId(i as u32), r).unwrap();
    }
    index.commit().unwrap();
    let mut rng = Lcg(0xADD5);
    let mut next = ROWS;
    let dir_bytes = support::dir_bytes;

    let mut g = c.benchmark_group("commit_10_rows");
    g.sample_size(10);
    let growth_before = dir_bytes(&dir);
    let written_before = index.bytes_written();
    g.bench_function("v2_append", |b| {
        b.iter(|| {
            for _ in 0..10 {
                index.add(DocId(next), &rng.vector(DIM)).unwrap();
                next += 1;
            }
            index.commit().unwrap();
        })
    });
    let commits = u64::from(next - ROWS) / 10;
    eprintln!(
        "v2_append: {} bytes written per 10-row commit (the handle's own count: rows + manifest; {} commits), net dense/ growth {} bytes per commit",
        (index.bytes_written() - written_before) / commits,
        commits,
        (dir_bytes(&dir) - growth_before) / commits
    );
    g.finish();
}

criterion_group!(benches, bench_scan, bench_commit);
criterion_main!(benches);
