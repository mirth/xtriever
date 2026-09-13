//! US1 scenario 3 — bit-identity across call composition, order and (across processes) thread
//! counts (spec FR-005, SC-002; research D5). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Budget, DocId, Passage, Reranker};
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder};

fn load() -> MiniLmCrossEncoder {
    MiniLmCrossEncoder::load(&support::model_dir(), LoadPath::Buffered).expect("load pinned model")
}

/// Score every golden pair through `rerank`, one call per query, as the pipeline would.
fn bits_per_query(r: &MiniLmCrossEncoder) -> Vec<Vec<u32>> {
    support::goldens()
        .queries
        .iter()
        .map(|q| {
            let passages: Vec<Passage<'_>> = q
                .passages
                .iter()
                .enumerate()
                .map(|(i, p)| Passage {
                    id: DocId(i as u32),
                    text: &p.text,
                })
                .collect();
            r.rerank(&q.query, &passages, &Budget::default())
                .unwrap()
                .into_iter()
                .map(|s| s.unwrap().to_bits())
                .collect()
        })
        .collect()
}

#[test]
#[ignore = "needs the model"]
fn three_arrangements_are_bit_identical() {
    let r = load();
    let g = support::goldens();
    for q in &g.queries {
        let texts: Vec<&str> = q.passages.iter().map(|p| p.text.as_str()).collect();
        let passages: Vec<Passage<'_>> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Passage {
                id: DocId(i as u32),
                text: t,
            })
            .collect();
        // (a) one call
        let one: Vec<u32> = r
            .rerank(&q.query, &passages, &Budget::default())
            .unwrap()
            .into_iter()
            .map(|s| s.unwrap().to_bits())
            .collect();
        // (b) one pair per `score`
        let singly: Vec<u32> = texts
            .iter()
            .map(|t| r.score(&q.query, t).unwrap().to_bits())
            .collect();
        // (c) reversed order in one call
        let mut rev = passages.clone();
        rev.reverse();
        let mut reversed: Vec<u32> = r
            .rerank(&q.query, &rev, &Budget::default())
            .unwrap()
            .into_iter()
            .map(|s| s.unwrap().to_bits())
            .collect();
        reversed.reverse();
        assert_eq!(one, singly, "{}: one call vs single scores", q.name);
        assert_eq!(one, reversed, "{}: one call vs reversed", q.name);
    }
}

/// Child process: score the golden set and print the bits. Only runs when spawned by the parent.
#[test]
#[ignore = "needs the model"]
fn thread_child() {
    if std::env::var_os("XT_THREAD_CHILD").is_none() {
        return;
    }
    let r = load();
    println!(
        "XT_BITS {} {}",
        MiniLmCrossEncoder::thread_count(),
        serde_json::to_string(&bits_per_query(&r)).unwrap()
    );
}

/// FR-005's thread-count clause, in the 004 cross-process style: candle sizes its pool from
/// `RAYON_NUM_THREADS` at process start, so each count needs its own process.
#[test]
#[ignore = "needs the model"]
fn scores_are_bit_identical_across_thread_counts() {
    let exe = std::env::current_exe().expect("test binary path");
    let mut outputs = Vec::new();
    for threads in ["1", "4"] {
        let out = std::process::Command::new(&exe)
            .args(["--exact", "thread_child", "--nocapture", "--ignored"])
            .env("XT_THREAD_CHILD", "1")
            .env("RAYON_NUM_THREADS", threads)
            .output()
            .expect("spawn child");
        assert!(
            out.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout
            .lines()
            .find(|l| l.starts_with("XT_BITS "))
            .expect("child printed bits");
        let mut parts = line["XT_BITS ".len()..].splitn(2, ' ');
        let reported: usize = parts.next().unwrap().parse().unwrap();
        assert_eq!(reported.to_string(), threads);
        let bits: Vec<Vec<u32>> = serde_json::from_str(parts.next().unwrap()).unwrap();
        outputs.push(bits);
    }
    assert_eq!(
        outputs[0], outputs[1],
        "RAYON_NUM_THREADS=1 and =4 produced different bits (research D5 — stop and report)"
    );
}
