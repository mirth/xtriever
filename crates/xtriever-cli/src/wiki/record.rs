//! The corpus identity (`<index>/corpus.json`), the build record (`wiki-build.json`) and the
//! attribution file (data-model "Corpus identity", "Build record", "Attribution"; research D12).
//! Canonical JSON — sorted keys, no whitespace, non-ASCII kept — is what the identity hashes.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::Manifest;
use super::rules::Rule;

/// Which snapshot the corpus came from.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotRef {
    /// Wikipedia edition.
    pub edition: String,
    /// Snapshot date.
    pub snapshot_date: String,
    /// The parquet's SHA-256.
    pub parquet_sha256: String,
    /// The derived JSONL's SHA-256.
    pub jsonl_sha256: String,
}

/// How the passages were cut.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChunkerRef {
    /// Algorithm version (contracts/chunker.md).
    pub version: u32,
    /// The budget rule, as text.
    pub budget: String,
    /// The cost function, as text.
    pub cost: String,
}

/// Counts of what went in and what was left out.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Counts {
    /// Articles read from the snapshot.
    pub articles: u64,
    /// Excluded, per rule (rule text → count).
    pub excluded: BTreeMap<String, u64>,
    /// Articles that produced passages.
    pub selected: u64,
    /// Passages indexed.
    pub passages: u64,
    /// Passages found over the window by the verify pass (must be 0).
    pub passages_over_window: u64,
    /// Articles whose derived URL differed from the snapshot's (must be 0).
    pub url_mismatches: u64,
}

/// `<index>/corpus.json`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CorpusIdentity {
    /// Layout version of this file.
    pub schema_version: u32,
    /// SHA-256 of the canonical JSON of `snapshot`, `exclusions`, `chunker`,
    /// `embedder_fingerprint` (and `partial`, when set).
    pub corpus_identity: String,
    /// The snapshot.
    pub snapshot: SnapshotRef,
    /// The exclusion rules, verbatim.
    pub exclusions: Vec<Rule>,
    /// The chunker.
    pub chunker: ChunkerRef,
    /// `Embedder::fingerprint()`.
    pub embedder_fingerprint: String,
    /// Set by `--limit N`: this is not the whole edition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<u64>,
    /// Set by `--sparse-encoder` (Feature 027): the index carries a sparse expansion, so it is
    /// another artefact than the same snapshot built without it. Part of the identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse: Option<SparseRef>,
    /// The counts (not part of the identity).
    pub counts: Counts,
}

/// What a sparse artefact's expansion is (Feature 027): the encoder's identity and the option.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SparseRef {
    /// `SparseEncoder::identity()`.
    pub encoder: String,
    /// As created.
    pub scale: u32,
    /// As created.
    pub boost: f32,
}

impl CorpusIdentity {
    /// Build the identity for a manifest, an embedder and an optional limit; counts empty.
    #[must_use]
    pub fn new(manifest: &Manifest, embedder_fingerprint: &str, partial: Option<u64>) -> Self {
        let mut id = Self {
            schema_version: 1,
            corpus_identity: String::new(),
            snapshot: SnapshotRef {
                edition: manifest.edition.clone(),
                snapshot_date: manifest.snapshot_date.clone(),
                parquet_sha256: manifest.parquet.sha256.clone(),
                jsonl_sha256: manifest.jsonl.sha256.clone(),
            },
            exclusions: manifest.exclusions.clone(),
            chunker: ChunkerRef {
                version: 1,
                budget: "256 - token_count(title)".to_owned(),
                cost: "MiniLmEmbedder::token_count(unit) - 2".to_owned(),
            },
            embedder_fingerprint: embedder_fingerprint.to_owned(),
            partial,
            sparse: None,
            counts: Counts::default(),
        };
        id.corpus_identity = id.compute_identity();
        id
    }

    /// The same identity for a sparse build (Feature 027): the expansion joins the basis, so a
    /// sparse artefact never shares its identity with the plain one.
    #[must_use]
    pub fn with_sparse(mut self, sparse: SparseRef) -> Self {
        self.sparse = Some(sparse);
        self.corpus_identity = self.compute_identity();
        self
    }

    fn compute_identity(&self) -> String {
        let mut basis = serde_json::json!({
            "snapshot": self.snapshot,
            "exclusions": self.exclusions,
            "chunker": self.chunker,
            "embedder_fingerprint": self.embedder_fingerprint,
        });
        if let Some(n) = self.partial {
            basis["partial"] = serde_json::json!(n);
        }
        if let Some(sparse) = &self.sparse {
            basis["sparse"] = serde_json::json!(sparse);
        }
        let canonical = canonical_json(&basis);
        Sha256::digest(canonical.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// Canonical JSON: keys sorted, no whitespace, non-ASCII unescaped — byte-identical to Python's
/// `json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`.
#[must_use]
pub fn canonical_json(value: &serde_json::Value) -> String {
    fn write(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::String((*k).clone()).to_string());
                    out.push(':');
                    write(&map[*k], out);
                }
                out.push('}');
            }
            serde_json::Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut out = String::new();
    write(value, &mut out);
    out
}

/// `wiki-build.json`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BuildRecord {
    /// Layout version.
    pub schema_version: u32,
    /// `008-wiki-corpus`.
    pub feature: String,
    /// RFC 3339 UTC.
    pub recorded_at: String,
    /// As in `corpus.json`.
    pub corpus_identity: String,
    /// The host.
    pub host: Host,
    /// Model identities.
    pub models: Models,
    /// The counts.
    pub counts: Counts,
    /// Wall time per phase.
    pub phases_ms: BTreeMap<String, u64>,
    /// The embedding cache's part.
    pub embedding_cache: CacheStats,
    /// Sizes of the artefact's files.
    pub artefact_bytes: BTreeMap<String, u64>,
    /// The verify pass.
    pub verify: Verify,
    /// The sparse expansion's build (Feature 027; sparse builds only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sparse: Option<SparseBuild>,
}

/// What a sparse build cost and did (Feature 027).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SparseBuild {
    /// The encoder, the scale and the boost (as in the identity).
    #[serde(flatten)]
    pub expansion: SparseRef,
    /// Passages expanded from a window truncated to the encoder's 512 tokens.
    pub truncated: u64,
    /// Wall time spent encoding, in milliseconds.
    pub encode_ms: u64,
    /// Passages encoded per second.
    pub passages_per_second: f64,
}

/// The build host.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Host {
    /// `std::env::consts::OS`.
    pub os: String,
    /// `std::env::consts::ARCH`.
    pub arch: String,
    /// candle's effective thread count.
    pub threads: usize,
}

/// Model identities the artefact depends on / is measured with.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Models {
    /// The embedder's fingerprint (part of the identity).
    pub embedder: String,
    /// The pinned re-ranker the demo attaches (not part of the identity).
    pub reranker_for_demo: String,
}

/// Cache hits and misses.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CacheStats {
    /// Where.
    pub dir: String,
    /// Shards seen.
    pub shards: u64,
    /// Shards read from the cache.
    pub hits: u64,
    /// Shards embedded and written.
    pub misses: u64,
}

/// The verify pass's verdict.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Verify {
    /// Passages tokenised.
    pub passages_checked: u64,
    /// The longest passage seen, in positions.
    pub max_tokens_seen: u64,
    /// The window.
    pub window: u64,
    /// `PASS` or `FAIL`.
    pub verdict: String,
}

/// The attribution text the app shows.
#[must_use]
pub fn attribution(manifest: &Manifest, corpus_identity: &str, recorded_at: &str) -> String {
    format!(
        "Text from Simple English Wikipedia, snapshot {} ({}).\n\
         Licensed under the Creative Commons Attribution-ShareAlike 4.0 licence ({}).\n\
         Each passage links to its source article; the title line of every passage is the article's.\n\
         Corpus identity {corpus_identity}, built {recorded_at}.\n",
        manifest.snapshot_date, manifest.source, manifest.licence.url
    )
}

/// RFC 3339 UTC, seconds — from `SystemTime`, no clock crate.
#[must_use]
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil-from-days (Howard Hinnant's algorithm), integer arithmetic only.
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Total bytes of every file under `dir`, keyed by path relative to `dir`, plus `total`.
///
/// # Errors
///
/// I/O errors.
pub fn artefact_bytes(dir: &Path) -> anyhow::Result<BTreeMap<String, u64>> {
    let mut out = BTreeMap::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).with_context(|| format!("listing {}", d.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let len = entry.metadata()?.len();
                total += len;
                let rel = path
                    .strip_prefix(dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, len);
            }
        }
    }
    out.insert("total".to_owned(), total);
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::wiki::manifest::Manifest;

    use std::path::Path;

    #[test]
    fn canonical_json_matches_python_json_dumps() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"b": [1, 2.5, "é", null, true], "a": {"z": "x\ny", "y": {}}}"#,
        )
        .unwrap();
        // python3 -c 'import json; print(json.dumps(json.loads(...), sort_keys=True, separators=(",", ":"), ensure_ascii=False))'
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"y":{},"z":"x\ny"},"b":[1,2.5,"é",null,true]}"#
        );
    }

    /// Feature 027: a sparse build is another artefact — its expansion joins the identity —
    /// while a plain build's identity is what it was.
    #[test]
    fn a_sparse_build_has_its_own_identity() {
        let m = Manifest::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/datasets/wiki-manifest.json"),
        )
        .unwrap();
        let plain = CorpusIdentity::new(&m, "fp-1", None);
        let sparse = |scale| SparseRef {
            encoder: "encoder@rev".into(),
            scale,
            boost: 1.0,
        };
        let a = CorpusIdentity::new(&m, "fp-1", None).with_sparse(sparse(10));
        assert_ne!(a.corpus_identity, plain.corpus_identity);
        assert_ne!(
            a.corpus_identity,
            CorpusIdentity::new(&m, "fp-1", None)
                .with_sparse(sparse(20))
                .corpus_identity,
            "scale"
        );
        let text = serde_json::to_string(&plain).unwrap();
        assert!(!text.contains("sparse"), "{text}");
    }

    #[test]
    fn the_identity_is_a_pure_function_of_its_inputs() {
        let m = Manifest::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference/datasets/wiki-manifest.json"),
        )
        .unwrap();
        let a = CorpusIdentity::new(&m, "fp-1", None);
        let b = CorpusIdentity::new(&m, "fp-1", None);
        assert_eq!(a.corpus_identity, b.corpus_identity);
        assert_eq!(a.corpus_identity.len(), 64);
        assert_ne!(
            a.corpus_identity,
            CorpusIdentity::new(&m, "fp-2", None).corpus_identity,
            "embedder"
        );
        assert_ne!(
            a.corpus_identity,
            CorpusIdentity::new(&m, "fp-1", Some(2000)).corpus_identity,
            "partial"
        );
        let mut m2 = m.clone();
        m2.exclusions.pop();
        assert_ne!(
            a.corpus_identity,
            CorpusIdentity::new(&m2, "fp-1", None).corpus_identity,
            "rules"
        );
        let mut m3 = m.clone();
        m3.jsonl.sha256 = "0".repeat(64);
        assert_ne!(
            a.corpus_identity,
            CorpusIdentity::new(&m3, "fp-1", None).corpus_identity,
            "snapshot"
        );
        // Counts are not part of it.
        let mut c = a.clone();
        c.counts.passages = 7;
        assert_eq!(
            serde_json::to_value(&c).unwrap()["corpus_identity"],
            serde_json::to_value(&a).unwrap()["corpus_identity"]
        );
    }

    #[test]
    fn rfc3339_now_has_the_shape() {
        let s = now_rfc3339();
        assert_eq!(s.len(), 20, "{s}");
        assert!(
            s.starts_with("20") && s.ends_with('Z') && &s[10..11] == "T",
            "{s}"
        );
    }
}
