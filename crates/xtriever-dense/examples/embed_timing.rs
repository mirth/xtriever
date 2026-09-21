//! Feature 026: how long one embedding takes, in the arithmetic the engine actually runs.
//!
//! ```sh
//! cargo run --release -p xtriever-dense --example embed_timing -- <model dir> [texts]
//! ```
//!
//! The directory decides what is timed, and nothing else does: float weights run candle's float
//! kernel, and the eight-bit artefact is expanded to `f16` at load and run through the same
//! kernel (ADR-0015). The encoder constructs each matmul as `QMatMul::TensorF16` itself rather
//! than through candle's `from_arc`, so candle 0.9.2's `CANDLE_DEQUANTIZE_ALL` and
//! `CANDLE_DEQUANTIZE_ALL_F16` switches cannot change a number this prints — the mode
//! comparison that chose `f16` (442 ms per embedding on the eight-bit kernel, 120 in `f16`, 123
//! in `f32`, 125 float) was measured with this tool while those switches still governed the
//! encoder, before the mode was fixed in code. Loading is excluded from the timing; the first
//! embedding is discarded as a warm-up. A record for the report, not a benchmark the feature
//! claims.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::path::PathBuf;
use std::time::Instant;

use xtriever_core::{Embedder, TextKind};
use xtriever_dense::{LoadPath, MiniLmEmbedder};

fn main() {
    let dir: PathBuf = std::env::args().nth(1).expect("model dir").into();
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let texts: Vec<String> = (0..n)
        .map(|i| format!("passage number {i}: the quick brown fox jumps over the lazy dog, again and again, {}", "lorem ipsum ".repeat(i % 7 + 3)))
        .collect();
    let started = Instant::now();
    let embedder = MiniLmEmbedder::load(&dir, LoadPath::Buffered).expect("load");
    println!(
        "loaded {:?} in {:.2} s (threads {})",
        embedder.precision(),
        started.elapsed().as_secs_f64(),
        MiniLmEmbedder::thread_count()
    );
    embedder
        .embed(&[texts[0].as_str()], TextKind::Passage)
        .unwrap();
    let started = Instant::now();
    for text in &texts {
        embedder.embed(&[text.as_str()], TextKind::Passage).unwrap();
    }
    let per = started.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!(
        "{n} embeddings: {per:.1} ms each ({:.2} per second)",
        1000.0 / per
    );
}
