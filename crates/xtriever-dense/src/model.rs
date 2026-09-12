//! The pinned model: identity, file pins and the fingerprint (research D4, D6).
//!
//! Every value here is a literal so the fingerprint can be assembled at compile time from the
//! same pieces as the pins — the two cannot disagree. `reference/models/manifest.json` carries the
//! same pins for `scripts/fetch-model.sh`; `tests/model_pins.rs` keeps them equal.

use std::path::Path;

use crate::error::model_err;

/// One pinned model file: name, exact size and SHA-256 (spec FR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile {
    /// File name inside the model directory.
    pub name: &'static str,
    /// Exact size in bytes.
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the whole file.
    pub sha256: &'static str,
}

/// The pinned model (spec FR-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedModel {
    /// Hugging Face repository.
    pub repository: &'static str,
    /// Git revision the files were fetched at.
    pub revision: &'static str,
    /// `config.json`, `tokenizer.json`, `model.safetensors`, in verification order.
    pub files: [PinnedFile; 3],
    /// Output dimensionality.
    pub dim: usize,
    /// Maximum input length in model tokens (the sentence-transformers setting, not the
    /// tokenizer file's 128 — 001 research D6).
    pub max_tokens: usize,
    /// Number of `F32` tensors in the safetensors header (001 FR-033).
    pub f32_tensors: usize,
}

// Literal pieces, as macros so `concat!` can see them (it accepts literals only).
macro_rules! repository {
    () => {
        "sentence-transformers/all-MiniLM-L6-v2"
    };
}
macro_rules! revision {
    () => {
        "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
    };
}
macro_rules! weights_sha256 {
    () => {
        "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db"
    };
}

/// The model this crate embeds with.
pub const PINNED: PinnedModel = PinnedModel {
    repository: repository!(),
    revision: revision!(),
    files: [
        PinnedFile {
            name: "config.json",
            bytes: 612,
            sha256: "953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41",
        },
        PinnedFile {
            name: "tokenizer.json",
            bytes: 466_247,
            sha256: "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
        },
        PinnedFile {
            name: "model.safetensors",
            bytes: 90_868_376,
            sha256: weights_sha256!(),
        },
    ],
    dim: 384,
    max_tokens: 256,
    f32_tensors: 103,
};

/// Short model name used in `Error::Model { model, .. }`.
pub const MODEL_NAME: &str = "all-MiniLM-L6-v2";

/// The embedder fingerprint (spec FR-004, research D6): every input whose change would change
/// the vectors — model identity, pooling, normalisation, truncation length, weight precision,
/// prefixes (none) and the inference engine version (ADR-0001). Not included: thread count and
/// CPU architecture — same fingerprint means bit-identical vectors *per architecture* (D3).
pub const FINGERPRINT: &str = concat!(
    repository!(),
    "@",
    revision!(),
    ";weights=sha256:",
    weights_sha256!(),
    ";dim=384;pool=mean-mask;norm=l2;max_tokens=256;dtype=f32;prefix=none;engine=candle-0.9.2"
);

/// Verify every pinned file's size and SHA-256 in `dir`, in order; the first failure wins.
///
/// Size is checked before content and the hash is streamed in 1 MiB chunks, so verifying the
/// 90.9 MB weights never allocates the file — the spike's method, kept so verification does not
/// inflate the footprint being measured (research D4). Nothing is parsed here.
///
/// # Errors
///
/// `Error::Model` naming the file and both sizes or both hashes (spec FR-003).
pub fn verify_files(dir: &Path) -> xtriever_core::Result<()> {
    for pin in &PINNED.files {
        verify_file(dir, pin)?;
    }
    Ok(())
}

fn verify_file(dir: &Path, pin: &PinnedFile) -> xtriever_core::Result<()> {
    use sha2::{Digest, Sha256};

    let path = dir.join(pin.name);
    let size = std::fs::metadata(&path)
        .map_err(|e| model_err(format!("cannot stat {}: {e}", path.display())))?
        .len();
    if size != pin.bytes {
        return Err(model_err(format!(
            "{} is {size} bytes, expected exactly {} bytes",
            path.display(),
            pin.bytes
        )));
    }
    let mut file = std::fs::File::open(&path)
        .map_err(|e| model_err(format!("cannot open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let read = std::io::Read::read(&mut file, &mut buf)
            .map_err(|e| model_err(format!("cannot hash {}: {e}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != pin.sha256 {
        return Err(model_err(format!(
            "{} has sha256 {digest}, expected {}",
            path.display(),
            pin.sha256
        )));
    }
    Ok(())
}
