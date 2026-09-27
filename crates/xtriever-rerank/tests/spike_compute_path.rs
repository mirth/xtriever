//! Feature 028 (the accelerated inference spike): the compute path is chosen at compile time by
//! a non-default feature, named by `spike::COMPUTE_PATH`, and never changes the re-ranker's
//! identity (research D5, D6). Spike code — removed or promoted by the follow-up the verdict
//! names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use xtriever_core::Reranker;
use xtriever_rerank::model::MODEL_ID_Q8;
use xtriever_rerank::{LoadPath, MiniLmCrossEncoder, spike};

#[test]
#[cfg(not(any(feature = "spike-accelerate", feature = "spike-metal")))]
fn the_default_build_is_the_cpu_path() {
    assert_eq!(spike::COMPUTE_PATH, "cpu");
}

#[test]
#[cfg(feature = "spike-accelerate")]
fn spike_accelerate_is_named() {
    assert_eq!(spike::COMPUTE_PATH, "accelerate");
}

#[test]
#[cfg(feature = "spike-metal")]
fn spike_metal_is_named() {
    assert_eq!(spike::COMPUTE_PATH, "metal");
}

#[test]
#[ignore = "needs the model"]
fn the_reranker_loads_on_the_path_with_todays_identity() {
    let reranker = MiniLmCrossEncoder::load(&support::model_dir_q8(), LoadPath::Buffered)
        .unwrap_or_else(|e| panic!("load on the {} path: {e}", spike::COMPUTE_PATH));
    assert_eq!(reranker.model_id(), MODEL_ID_Q8);
}
