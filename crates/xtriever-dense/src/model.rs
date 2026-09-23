//! The pinned models: identity, file pins and the fingerprints (research D4, D6; Feature 026
//! research D7, D8).
//!
//! Every value here is a literal so a fingerprint can be assembled at compile time from the
//! same pieces as the pins — the two cannot disagree. `reference/models/manifest.json` and
//! `manifest-q8.json` carry the same pins for `scripts/fetch-model.sh`; `tests/model_pins.rs`
//! keeps them equal.
//!
//! Two artefacts are pinned (Feature 026, ADR-0015): the as-published float weights and the
//! owner-supplied eight-bit GGUF. A model directory holds one or the other, and the loader tells
//! them apart by which weights file is present; the eight-bit directory carries the float
//! model's `config.json` and `tokenizer.json` beside its weights (the artefact supplies weights
//! only), verified against the same pins.

use std::path::Path;

use crate::error::model_err;
use crate::gguf_header::{BertPin, GgufHeader};
// The arithmetic's one literal, defined beside the matmul it selects (`quantised_bert`).
use crate::quantised_bert::compute;

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

/// Which artefact a loaded embedder came from (Feature 026, spec FR-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// `model.safetensors`, the as-published float weights ([`PINNED`]).
    Float,
    /// The eight-bit GGUF the owner pinned ([`PINNED_Q8`]); the weight matrices are eight-bit
    /// blocks, norms, biases and token types stay float, and the tokenizer is the float model's.
    EightBit,
}

/// The eight-bit artefact: weights in one GGUF file, plus the float model's configuration and
/// tokenizer beside it (Feature 026 research D5–D7; `reference/models/manifest-q8.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedArtefact {
    /// Hugging Face repository of the eight-bit file.
    pub repository: &'static str,
    /// Git revision the file was fetched at.
    pub revision: &'static str,
    /// `config.json`, `tokenizer.json` (both the float model's pins) and the GGUF, in
    /// verification order.
    pub files: [PinnedFile; 3],
    /// The block format the weight matrices use, as the file's `general.file_type` names it.
    pub quantisation: &'static str,
    /// What the GGUF header must declare: architecture, block count, embedding length, head
    /// count, feed-forward length and context length — the shape the forward pass is written
    /// for (research D5).
    pub architecture: &'static str,
    /// What `bert.block_count` must say.
    pub blocks: usize,
    /// What `bert.embedding_length` must say: the hidden size.
    pub embedding_length: usize,
    /// What `bert.attention.head_count` must say.
    pub heads: usize,
    /// What `bert.feed_forward_length` must say.
    pub feed_forward_length: usize,
    /// What `bert.context_length` must say, exactly: the encoder sizes its position table
    /// from it (the window the stage feeds is at most this).
    pub context_length: usize,
    /// How many tensors the file stores in eight-bit blocks (the weight matrices).
    pub quantised_tensors: usize,
}

macro_rules! q8_repository {
    () => {
        "leliuga/all-MiniLM-L6-v2-GGUF"
    };
}
macro_rules! q8_revision {
    () => {
        "ddf2e25d5b8530422e7b14aa39f33a657ff9aec0"
    };
}
macro_rules! q8_weights_sha256 {
    () => {
        "e5ec722e8c82dc4ffaf965175ca472f5da3f97b695590b5b0780bdbfa29bcaf3"
    };
}

/// The eight-bit artefact this crate embeds with when the model directory holds it.
pub const PINNED_Q8: PinnedArtefact = PinnedArtefact {
    repository: q8_repository!(),
    revision: q8_revision!(),
    files: [
        PINNED.files[0],
        PINNED.files[1],
        PinnedFile {
            name: "all-MiniLM-L6-v2.Q8_0.gguf",
            bytes: 25_008_064,
            sha256: q8_weights_sha256!(),
        },
    ],
    quantisation: "q8_0",
    architecture: "bert",
    blocks: 6,
    embedding_length: 384,
    heads: 12,
    feed_forward_length: 1536,
    context_length: 512,
    quantised_tensors: 37,
};

/// The embedder fingerprint for the eight-bit artefact: the same inputs as [`FINGERPRINT`] with
/// the artefact and `dtype=q8_0` in place of the float file, plus `compute=f32`, the arithmetic
/// the matrices are multiplied in (`compute!()`) — a different arithmetic would be
/// a different vector — so an index records which weights produced it and a float index refuses
/// to open with this embedder (spec FR-006, FR-007).
pub const FINGERPRINT_Q8: &str = concat!(
    q8_repository!(),
    "@",
    q8_revision!(),
    ";weights=sha256:",
    q8_weights_sha256!(),
    ";dim=384;pool=mean-mask;norm=l2;max_tokens=256;dtype=q8_0;compute=",
    compute!(),
    ";prefix=none;engine=candle-0.9.2"
);

/// The sparse document encoder (Feature 027 research D1, D3): repository, revision and every
/// file it needs — the model, its tokenizer and the query-side table (`idf.json`). Mirrors
/// `reference/models/manifest-sparse-doc-v3.json`; `tests/sparse_pins.rs` keeps them equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedSparseEncoder {
    /// Hugging Face repository.
    pub repository: &'static str,
    /// Git revision the files were fetched at.
    pub revision: &'static str,
    /// `config.json`, `tokenizer.json`, `model.safetensors`, `idf.json`, in verification order.
    pub files: [PinnedFile; 4],
    /// The activation the document weights go through, as the manifest names it.
    pub activation: &'static str,
    /// The encoder's window in model tokens, special tokens included.
    pub max_tokens: usize,
}

macro_rules! sparse_repository {
    () => {
        "opensearch-project/opensearch-neural-sparse-encoding-doc-v3-distill"
    };
}
macro_rules! sparse_revision {
    () => {
        "babf71f3c48695e2e53a978208e8aba48335e3c0"
    };
}
macro_rules! sparse_weights_sha256 {
    () => {
        "83a3cc9757876b8590aac53f4f6685012f89d7fb4bbeb540815a54d325f7f70a"
    };
}

/// The sparse document encoder this crate expands documents with (Feature 027).
pub const PINNED_SPARSE: PinnedSparseEncoder = PinnedSparseEncoder {
    repository: sparse_repository!(),
    revision: sparse_revision!(),
    files: [
        PinnedFile {
            name: "config.json",
            bytes: 596,
            sha256: "ee97780493e7d0a3b7b788ea98f3391e6be6b0b379921b465ca55bfdd0d9cbe3",
        },
        PinnedFile {
            name: "tokenizer.json",
            bytes: 711_649,
            sha256: "91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854",
        },
        PinnedFile {
            name: "model.safetensors",
            bytes: 267_954_768,
            sha256: sparse_weights_sha256!(),
        },
        PinnedFile {
            name: "idf.json",
            bytes: 889_360,
            sha256: "da23a1c0b9252776cc8c6d70fd14723e218f484d489cd9027ac6e4065d5b9edd",
        },
    ],
    activation: "log1p_log1p_relu",
    max_tokens: 512,
};

/// Short name used in the sparse encoder's `Error::Model { model, .. }`.
pub const SPARSE_MODEL_NAME: &str = "opensearch-neural-sparse-encoding-doc-v3-distill";

/// The sparse encoder's identity, recorded in a sparse index (Feature 027 data-model
/// `SparseRecord.encoder`): every input whose change would change a document's expansion.
pub const SPARSE_IDENTITY: &str = concat!(
    sparse_repository!(),
    "@",
    sparse_revision!(),
    ";weights=sha256:",
    sparse_weights_sha256!(),
    ";activation=log1p_log1p_relu;max_tokens=512;engine=candle-0.9.2"
);

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
        verify_file(dir, pin, model_err)?;
    }
    Ok(())
}

/// Verify the eight-bit directory's files ([`PINNED_Q8`]) the same way.
///
/// # Errors
///
/// `Error::Model` naming the file and both sizes or both hashes.
pub fn verify_files_q8(dir: &Path) -> xtriever_core::Result<()> {
    for pin in &PINNED_Q8.files {
        verify_file(dir, pin, model_err)?;
    }
    Ok(())
}

/// Assert what an eight-bit artefact's GGUF header declares against [`PINNED_Q8`]: the
/// architecture, block count, embedding length, head count, feed-forward length, context
/// length and the number of eight-bit tensors. The bytes were verified by [`verify_files_q8`];
/// this guards the pin itself — a pinned file that is not the model the forward pass is written
/// for is refused by name, not run. The assertion is the header reader's, shared with the
/// re-ranker byte for byte.
///
/// # Errors
///
/// `Error::Model` naming the field and both values.
pub fn assert_gguf_header(bytes: &[u8]) -> xtriever_core::Result<()> {
    checked_gguf_header(bytes).map(|_| ())
}

/// [`assert_gguf_header`], returning the header it checked so a loader parses the file once.
pub(crate) fn checked_gguf_header(bytes: &[u8]) -> xtriever_core::Result<GgufHeader> {
    let header = GgufHeader::read(bytes)?;
    header.assert_bert(&bert_pin())?;
    Ok(header)
}

/// The part of [`PINNED_Q8`] the header reader asserts.
fn bert_pin() -> BertPin {
    BertPin {
        architecture: PINNED_Q8.architecture,
        blocks: PINNED_Q8.blocks,
        embedding_length: PINNED_Q8.embedding_length,
        heads: PINNED_Q8.heads,
        feed_forward_length: PINNED_Q8.feed_forward_length,
        context_length: PINNED_Q8.context_length,
        required_tensors: &[],
        quantisation: PINNED_Q8.quantisation,
        quantised_tensors: PINNED_Q8.quantised_tensors,
    }
}

/// Read one pinned file whole through `load_path` and check **those bytes** — size, then SHA-256
/// — before returning them, so what the caller parses is exactly what was verified (Feature 027;
/// the sparse encoder's loader). `err` names the model whose file it is.
///
/// # Errors
///
/// `err(…)` naming the file and both sizes or both hashes, or why it could not be read.
pub(crate) fn read_pinned(
    dir: &Path,
    pin: &PinnedFile,
    load_path: crate::LoadPath,
    err: fn(String) -> xtriever_core::Error,
) -> xtriever_core::Result<crate::bytes::Bytes> {
    let path = dir.join(pin.name);
    // The size first, from the metadata: a wrong file is refused before it is read.
    check_size(&path, pin, err)?;
    let bytes = crate::bytes::read(&path, load_path)
        .map_err(|e| err(format!("cannot read {}: {e}", path.display())))?;
    let slice = bytes.as_slice();
    if slice.len() as u64 != pin.bytes {
        return Err(err(format!(
            "{} is {} bytes, expected exactly {} bytes",
            path.display(),
            slice.len(),
            pin.bytes
        )));
    }
    check_sha256(&path, &sha256_hex(slice), pin.sha256, err)?;
    Ok(bytes)
}

/// The SHA-256 of `bytes`, as lower-case hex.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(bytes))
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of a whole file, as lower-case hex, streamed in 1 MiB chunks so a large file is
/// never held in memory; `err` names the model whose file it is.
fn sha256_file(
    path: &Path,
    err: fn(String) -> xtriever_core::Error,
) -> xtriever_core::Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path)
        .map_err(|e| err(format!("cannot open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let read = std::io::Read::read(&mut file, &mut buf)
            .map_err(|e| err(format!("cannot hash {}: {e}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn check_size(
    path: &Path,
    pin: &PinnedFile,
    err: fn(String) -> xtriever_core::Error,
) -> xtriever_core::Result<()> {
    let size = std::fs::metadata(path)
        .map_err(|e| err(format!("cannot stat {}: {e}", path.display())))?
        .len();
    if size != pin.bytes {
        return Err(err(format!(
            "{} is {size} bytes, expected exactly {} bytes",
            path.display(),
            pin.bytes
        )));
    }
    Ok(())
}

/// `digest` (of `path`) against `expected`; the one mismatch message every hash check in the
/// crate gives, whichever error `err` raises.
pub(crate) fn check_sha256(
    path: &Path,
    digest: &str,
    expected: &str,
    err: fn(String) -> xtriever_core::Error,
) -> xtriever_core::Result<()> {
    if digest != expected {
        return Err(err(format!(
            "{} has sha256 {digest}, expected {expected}",
            path.display()
        )));
    }
    Ok(())
}

/// Verify one pinned file by size and a streamed SHA-256, holding none of it: for a file the
/// caller does not parse (the sparse encoder's query-side table).
pub(crate) fn verify_file(
    dir: &Path,
    pin: &PinnedFile,
    err: fn(String) -> xtriever_core::Error,
) -> xtriever_core::Result<()> {
    let path = dir.join(pin.name);
    check_size(&path, pin, err)?;
    check_sha256(&path, &sha256_file(&path, err)?, pin.sha256, err)
}
