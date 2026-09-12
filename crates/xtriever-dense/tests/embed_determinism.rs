//! US1 scenarios 4 and 7 — bit-identity across batch arrangements, text kinds, and (across
//! processes) thread counts (spec FR-005, FR-007, SC-002; research D2/D3). Model-backed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::{Embedder, TextKind};
use xtriever_dense::{LoadPath, MiniLmEmbedder};

fn load() -> MiniLmEmbedder {
    MiniLmEmbedder::load(&support::model_dir(), LoadPath::Buffered).expect("load pinned model")
}

fn golden_texts() -> Vec<String> {
    support::embeddings()
        .cases
        .into_iter()
        .map(|c| c.text)
        .collect()
}

#[test]
#[ignore = "needs the model"]
fn three_batch_arrangements_are_bit_identical() {
    let e = load();
    let texts = golden_texts();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();

    // (a) one batch
    let one_batch = e.embed(&refs, TextKind::Passage).unwrap();
    // (b) one text per call
    let singly: Vec<Vec<f32>> = refs
        .iter()
        .map(|t| e.embed(&[t], TextKind::Passage).unwrap().remove(0))
        .collect();
    // (c) reversed, in two batches
    let mut rev = refs.clone();
    rev.reverse();
    let (first, second) = rev.split_at(rev.len() / 2);
    let mut two: Vec<Vec<f32>> = e.embed(first, TextKind::Passage).unwrap();
    two.extend(e.embed(second, TextKind::Passage).unwrap());
    two.reverse();

    assert_eq!(one_batch.len(), refs.len());
    for (i, t) in refs.iter().enumerate() {
        assert_eq!(
            support::bits(&one_batch[i]),
            support::bits(&singly[i]),
            "text {i} ({t:.30}): batch vs single"
        );
        assert_eq!(
            support::bits(&one_batch[i]),
            support::bits(&two[i]),
            "text {i} ({t:.30}): batch vs reversed halves"
        );
    }
}

#[test]
#[ignore = "needs the model"]
fn query_and_passage_kinds_are_bit_identical() {
    let e = load();
    let texts = golden_texts();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let q = e.embed(&refs, TextKind::Query).unwrap();
    let p = e.embed(&refs, TextKind::Passage).unwrap();
    for i in 0..refs.len() {
        assert_eq!(support::bits(&q[i]), support::bits(&p[i]), "text {i}");
    }
}

#[test]
#[ignore = "needs the model"]
fn empty_batch_yields_empty_output() {
    let e = load();
    assert!(e.embed(&[], TextKind::Passage).unwrap().is_empty());
}

#[test]
#[ignore = "needs the model"]
fn duplicate_texts_in_one_batch_are_bit_identical() {
    let e = load();
    let out = e
        .embed(&["same text", "same text"], TextKind::Passage)
        .unwrap();
    assert_eq!(support::bits(&out[0]), support::bits(&out[1]));
}

/// Child process: embed the golden set and print the bits. Only runs when spawned by the parent.
#[test]
#[ignore = "needs the model"]
fn thread_child() {
    if std::env::var_os("XT_THREAD_CHILD").is_none() {
        return;
    }
    let e = load();
    let texts = golden_texts();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let out = e.embed(&refs, TextKind::Passage).unwrap();
    let bits: Vec<Vec<u32>> = out.iter().map(|v| support::bits(v)).collect();
    println!(
        "XT_BITS {} {}",
        MiniLmEmbedder::thread_count(),
        serde_json::to_string(&bits).unwrap()
    );
}

/// FR-005's thread-count clause, in the Feature 002 cross-process style: candle sizes its pool
/// from `RAYON_NUM_THREADS` at process start, so each count needs its own process.
#[test]
#[ignore = "needs the model"]
fn embeddings_are_bit_identical_across_thread_counts() {
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
        assert_eq!(
            reported.to_string(),
            threads,
            "child ran with the requested thread count"
        );
        let bits: Vec<Vec<u32>> = serde_json::from_str(parts.next().unwrap()).unwrap();
        outputs.push(bits);
    }
    assert_eq!(
        outputs[0], outputs[1],
        "RAYON_NUM_THREADS=1 and =4 produced different bits (research D3 — stop and report)"
    );
}
