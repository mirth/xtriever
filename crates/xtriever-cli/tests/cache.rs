//! `wiki::cache` — the sharded, resumable embedding cache (data-model "Embedding cache";
//! research D9). Bit-identical round trip; every mismatch is a miss; a `.tmp` is ignored.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtriever_cli::wiki::cache::{cache_dir_for, read_shard, shard_key, write_shard};

const FP: &str = "sentence-transformers/all-MiniLM-L6-v2@abc;dim=384";

#[test]
fn a_written_shard_reads_back_bit_identical() {
    let dir = tempfile::tempdir().unwrap();
    let key = shard_key(&["one", "two"]);
    let vectors: Vec<f32> = (0..2 * 3).map(|i| (i as f32) * 0.5 - 1.0).collect();
    write_shard(dir.path(), 7, &key, 2, 3, FP, &vectors).unwrap();
    assert_eq!(
        read_shard(dir.path(), 7, &key, 2, 3, FP),
        Some(vectors.clone())
    );
    let bits: Vec<u32> = read_shard(dir.path(), 7, &key, 2, 3, FP)
        .unwrap()
        .iter()
        .map(|f| f.to_bits())
        .collect();
    assert_eq!(
        bits,
        vectors.iter().map(|f| f.to_bits()).collect::<Vec<_>>()
    );
    assert!(dir.path().join("shard-00007.f32").exists());
    assert!(dir.path().join("shard-00007.json").exists());
    assert!(!dir.path().join("shard-00007.f32.tmp").exists());
}

#[test]
fn every_mismatch_is_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    let key = shard_key(&["a", "b", "c"]);
    let vectors = vec![1.0f32; 3 * 2];
    write_shard(dir.path(), 0, &key, 3, 2, FP, &vectors).unwrap();
    assert!(read_shard(dir.path(), 0, &key, 3, 2, FP).is_some());
    assert!(
        read_shard(dir.path(), 0, &shard_key(&["a", "b", "x"]), 3, 2, FP).is_none(),
        "key"
    );
    assert!(read_shard(dir.path(), 0, &key, 2, 2, FP).is_none(), "count");
    assert!(read_shard(dir.path(), 0, &key, 3, 3, FP).is_none(), "dim");
    assert!(
        read_shard(dir.path(), 0, &key, 3, 2, "other-model").is_none(),
        "fingerprint"
    );
    assert!(
        read_shard(dir.path(), 1, &key, 3, 2, FP).is_none(),
        "shard number"
    );
    // Truncated vectors file: a miss, not a panic.
    std::fs::write(dir.path().join("shard-00000.f32"), [0u8; 4]).unwrap();
    assert!(
        read_shard(dir.path(), 0, &key, 3, 2, FP).is_none(),
        "length"
    );
}

#[test]
fn a_leftover_tmp_is_ignored_and_the_key_is_content_addressed() {
    let dir = tempfile::tempdir().unwrap();
    let key = shard_key(&["x"]);
    std::fs::write(dir.path().join("shard-00002.f32.tmp"), [0u8; 8]).unwrap();
    std::fs::write(dir.path().join("shard-00002.json.tmp"), b"{}").unwrap();
    assert!(read_shard(dir.path(), 2, &key, 1, 2, FP).is_none());
    assert_eq!(shard_key(&["x"]), shard_key(&["x"]));
    assert_ne!(
        shard_key(&["x", "y"]),
        shard_key(&["xy"]),
        "texts are length-prefixed"
    );
    assert_ne!(
        shard_key(&["x", "y"]),
        shard_key(&["x\0y"]),
        "a NUL inside a text is content, not a boundary"
    );
    assert_eq!(shard_key(&[]).len(), 64);
}

#[test]
fn the_cache_directory_is_keyed_by_the_embedder_fingerprint() {
    let root = std::path::Path::new("/cache");
    let a = cache_dir_for(root, FP);
    let b = cache_dir_for(root, "different");
    assert_ne!(a, b);
    assert_eq!(a.parent(), Some(root));
    assert_eq!(a.file_name().unwrap().len(), 16);
}
