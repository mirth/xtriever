//! Feature 024 (spec FR-011, SC-001, SC-004; research D8): the exact scan over the version-2
//! row file against the version-1 shape, and the cost of a 10-row commit against the rewrite
//! version 1 did. 100,000 rows × 384 dims from a fixed-seed generator; `k = 10`.
//!
//! The version-1 shape is kept here as bench-local code (three columns, the same `f64`
//! accumulation and the same total order), so the comparison outlives the format's removal.
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
use std::io::Write;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use xtriever_core::{DocId, DocSet, Metric, VectorIndex};
use xtriever_dense::FlatIndex;

const ROWS: u32 = 100_000;
const DIM: usize = 384;
const K: usize = 10;

struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    fn vector(&mut self) -> Vec<f32> {
        (0..DIM)
            .map(|_| ((self.next_u64() >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0)
            .collect()
    }
}

fn rows() -> Vec<Vec<f32>> {
    let mut rng = Lcg(0x0024_BE4C);
    (0..ROWS).map(|_| rng.vector()).collect()
}

fn query() -> Vec<f32> {
    Lcg(0xC0FFEE).vector()
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

    /// The version-1 commit: the whole file rewritten (header · ids · norms · vectors).
    fn rewrite(&self, path: &std::path::Path) {
        let mut out = Vec::with_capacity(16 + 64 + self.ids.len() * 8 + self.vectors.len() * 4);
        out.extend_from_slice(b"XTDENSE1");
        let header = format!(
            r#"{{"format_version":1,"dim":{DIM},"metric":"cosine","fingerprint":"bench","count":{}}}"#,
            self.ids.len()
        );
        out.extend_from_slice(&(header.len() as u64).to_le_bytes());
        out.extend_from_slice(header.as_bytes());
        for id in &self.ids {
            out.extend_from_slice(&id.to_le_bytes());
        }
        for n in &self.norms {
            out.extend_from_slice(&n.to_le_bytes());
        }
        for x in &self.vectors {
            out.extend_from_slice(&x.to_le_bytes());
        }
        let tmp = path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(&out).unwrap();
        f.sync_all().unwrap();
        std::fs::rename(&tmp, path).unwrap();
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
    // Agreement, so the comparison is between equals.
    let a = columnar.search(&q, K);
    let b: Vec<(f32, u32)> = index
        .search(&q, None, K)
        .unwrap()
        .iter()
        .map(|h| (h.score, h.id.0))
        .collect();
    assert_eq!(a, b, "the two shapes must agree bit for bit");
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
    let columnar = Columnar::build(&rows);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("v2");
    let mut index = FlatIndex::create(&dir, DIM, Metric::Cosine, "bench").unwrap();
    for (i, r) in rows.iter().enumerate() {
        index.add(DocId(i as u32), r).unwrap();
    }
    index.commit().unwrap();
    let mut rng = Lcg(0xADD5);
    let mut next = ROWS;
    let v1_path = tmp.path().join("v1-index.bin");
    columnar.rewrite(&v1_path);
    let dir_bytes = |d: &std::path::Path| -> u64 {
        std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum()
    };

    let mut g = c.benchmark_group("commit_10_rows");
    g.sample_size(10);
    let before = dir_bytes(&dir);
    g.bench_function("v2_append", |b| {
        b.iter(|| {
            for _ in 0..10 {
                index.add(DocId(next), &rng.vector()).unwrap();
                next += 1;
            }
            index.commit().unwrap();
        })
    });
    let per_commit = (dir_bytes(&dir) - before) / u64::from(next - ROWS) * 10;
    eprintln!(
        "v2_append: ~{per_commit} bytes written per 10-row commit (dense/ grew by {} over {} rows)",
        dir_bytes(&dir) - before,
        next - ROWS
    );
    g.bench_function("v1_rewrite", |b| {
        b.iter(|| columnar.rewrite(black_box(&v1_path)))
    });
    eprintln!(
        "v1_rewrite: {} bytes written per commit",
        std::fs::metadata(&v1_path).unwrap().len()
    );
    g.finish();
}

criterion_group!(benches, bench_scan, bench_commit);
criterion_main!(benches);
