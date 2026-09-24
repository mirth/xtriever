//! Feature 027 (FR-012, FR-013): the packagers stage the index and the two pinned models, never
//! the sparse encoder — a device searches a sparse index with the query side the index carries.
//! A guard, like the pin tests: green at the red checkpoint, and it fails the day a packager
//! learns to stage the encoder. It reads the scripts, so an operator who deliberately points an
//! embedder override (`XTRIEVER_MODEL_MANIFEST`) at the encoder's manifest is outside it — that
//! app would fail to load the encoder as an embedder. Model-free; runs in CI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

#[test]
fn neither_packager_names_the_sparse_encoder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for script in [
        "scripts/build-ios-package.sh",
        "scripts/build-android-package.sh",
    ] {
        let text = std::fs::read_to_string(root.join(script))
            .unwrap_or_else(|e| panic!("read {script}: {e}"));
        for needle in [
            "manifest-sparse",
            "opensearch-neural-sparse",
            "sparse-encoder",
        ] {
            assert!(!text.contains(needle), "{script} names {needle}");
        }
    }
}
