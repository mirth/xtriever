//! The pinned models: identity, file pins and the identity strings (research D2; Feature 026
//! research D5–D8).
//!
//! Every value here is a literal so an identity string can be assembled at compile time from the
//! same pieces as the pins — the two cannot disagree. `reference/models/manifest-rerank.json`
//! and `manifest-rerank-q8.json` carry the same pins for `scripts/fetch-model.sh --manifest`;
//! `tests/model_pins.rs` keeps them equal.
//!
//! Two artefacts are pinned (Feature 026, ADR-0015): the as-published float weights and the
//! owner-supplied eight-bit GGUF. A model directory holds one or the other, told apart by which
//! weights file is present; the eight-bit directory carries the float model's `config.json` and
//! `tokenizer.json` beside its weights (the artefact supplies weights only).

use std::path::Path;

use crate::error::model_err;
use crate::gguf_header::{BertPin, GgufHeader};

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
    /// Hidden size (the pooler's width).
    pub hidden: usize,
    /// Maximum input length of the query–passage pair in model tokens, special tokens included.
    pub max_tokens: usize,
    /// Number of `F32` tensors in the safetensors header (105: the encoder, the pooler and the
    /// classifier; one further `I64` index buffer is not a weight).
    pub f32_tensors: usize,
}

// Literal pieces, as macros so `concat!` can see them (it accepts literals only).
macro_rules! repository {
    () => {
        "cross-encoder/ms-marco-MiniLM-L-6-v2"
    };
}
macro_rules! revision {
    () => {
        "233902d25c440f23af6f7d6e94d2946bac0bee0a"
    };
}
macro_rules! weights_sha256 {
    () => {
        "821d1aa69520101d6e0737f78a042ae25b19e5cb9160701909d10434f4aeb0ae"
    };
}

/// The model this crate scores with.
pub const PINNED: PinnedModel = PinnedModel {
    repository: repository!(),
    revision: revision!(),
    files: [
        PinnedFile {
            name: "config.json",
            bytes: 794,
            sha256: "380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc",
        },
        PinnedFile {
            name: "tokenizer.json",
            bytes: 711_396,
            sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
        },
        PinnedFile {
            name: "model.safetensors",
            bytes: 90_870_598,
            sha256: weights_sha256!(),
        },
    ],
    hidden: 384,
    max_tokens: 512,
    f32_tensors: 105,
};

/// Short model name used in `Error::Model { model, .. }`.
pub const MODEL_NAME: &str = "ms-marco-MiniLM-L-6-v2";

/// Which artefact a loaded cross-encoder came from (Feature 026, spec FR-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// `model.safetensors`, the as-published float weights ([`PINNED`]).
    Float,
    /// The eight-bit GGUF the owner pinned ([`PINNED_Q8`]).
    EightBit,
}

/// The eight-bit artefact: weights in one GGUF file, plus the float model's configuration and
/// tokenizer beside it (`reference/models/manifest-rerank-q8.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedArtefact {
    /// Hugging Face repository of the eight-bit file.
    pub repository: &'static str,
    /// Git revision the file was fetched at.
    pub revision: &'static str,
    /// `config.json`, `tokenizer.json` (the float model's pins), the GGUF, and
    /// `pooler.safetensors` — the pooler the artefact lacks, copied byte for byte out of the
    /// pinned float weights by `scripts/extract_tensors.py` (owner's decision, 2026-09-21).
    pub files: [PinnedFile; 4],
    /// The block format the weight matrices use, as the file's `general.file_type` names it.
    pub quantisation: &'static str,
    /// What the GGUF header must declare (research D5).
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
    /// The classification head's tensors, which the file must carry (spec FR-008): without
    /// them a cross-encoder produces embeddings that look like scores.
    pub classifier_tensors: [&'static str; 2],
}

macro_rules! q8_repository {
    () => {
        "cstr/ms-marco-MiniLM-L-6-v2-GGUF"
    };
}
macro_rules! q8_revision {
    () => {
        "1a9ef5ce8cb08936338233731314f3ff61ce0930"
    };
}
/// The arithmetic the eight-bit matrices are multiplied in (`quantised_bert`): expanded to
/// `f32` at load, multiplied by the float kernel — the owner's choice over candle's eight-bit
/// CPU kernel, which is 3.7× slower for the sequences this stage feeds, and over `f16`, which
/// did not hold parity across platforms (ADR-0015). A different arithmetic would be different
/// numbers, so the fingerprint names it.
macro_rules! compute {
    () => {
        "f32"
    };
}
macro_rules! q8_weights_sha256 {
    () => {
        "718e6861183047048bca4997ac2e03bd82babfc48c82f58e1a18b7d43136b15a"
    };
}
macro_rules! q8_pooler_sha256 {
    () => {
        "382eb35e6b806190f9781c0b3646090e696a9c4c6267eef88c4f70df949676b9"
    };
}

/// The eight-bit artefact this crate scores with when the model directory holds it.
pub const PINNED_Q8: PinnedArtefact = PinnedArtefact {
    repository: q8_repository!(),
    revision: q8_revision!(),
    files: [
        PINNED.files[0],
        PINNED.files[1],
        PinnedFile {
            name: "ms-marco-MiniLM-L-6-v2-q8_0.gguf",
            bytes: 24_703_040,
            sha256: q8_weights_sha256!(),
        },
        PinnedFile {
            name: "pooler.safetensors",
            bytes: 591_538,
            sha256: q8_pooler_sha256!(),
        },
    ],
    quantisation: "q8_0",
    architecture: "bert",
    blocks: 6,
    embedding_length: 384,
    heads: 12,
    feed_forward_length: 1536,
    context_length: 512,
    quantised_tensors: 38,
    classifier_tensors: ["classifier.weight", "classifier.bias"],
};

/// The identity string for the eight-bit artefact: [`MODEL_ID`]'s inputs with the artefact and
/// `dtype=q8_0` in place of the float file, `compute=f32` for the arithmetic the encoder's
/// matrices are multiplied in (`compute!()`), and the borrowed pooler named by its
/// own file's hash, since it too produced the score (spec FR-006).
pub const MODEL_ID_Q8: &str = concat!(
    q8_repository!(),
    "@",
    q8_revision!(),
    ";weights=sha256:",
    q8_weights_sha256!(),
    ";pooler=sha256:",
    q8_pooler_sha256!(),
    ";max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=q8_0;compute=",
    compute!(),
    ";engine=candle-0.9.2"
);

/// The model identity (spec FR-004, research D2): every input whose change would change a
/// score — model identity, truncation length and strategy, the head, the output activation,
/// the weight precision and the inference engine version. Not included: thread count and CPU
/// architecture — same identity means bit-identical scores *per architecture* (D5).
pub const MODEL_ID: &str = concat!(
    repository!(),
    "@",
    revision!(),
    ";weights=sha256:",
    weights_sha256!(),
    ";max_tokens=512;trunc=longest_first;head=cls-pooler-tanh-linear;act=identity;dtype=f32;engine=candle-0.9.2"
);

/// Verify every pinned file's size and SHA-256 in `dir`, in order; the first failure wins.
///
/// Size is checked before content and the hash is streamed in 1 MiB chunks, so verifying the
/// 90.9 MB weights never allocates the file (the 004 method). Nothing is parsed here.
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

/// Verify the eight-bit directory's files ([`PINNED_Q8`]) the same way.
///
/// # Errors
///
/// `Error::Model` naming the file and both sizes or both hashes.
pub fn verify_files_q8(dir: &Path) -> xtriever_core::Result<()> {
    for pin in &PINNED_Q8.files {
        verify_file(dir, pin)?;
    }
    Ok(())
}

/// Assert what an eight-bit artefact's GGUF header declares against [`PINNED_Q8`]: the
/// architecture and shape, the classification head's tensors by name (the re-ranker's own
/// requirement, spec FR-008) and the number of eight-bit tensors — the header reader's
/// assertion, shared with the embedder byte for byte. The bytes were verified by
/// [`verify_files_q8`]; this guards the pin itself.
///
/// # Errors
///
/// `Error::Model` naming the field and both values, or the missing tensor.
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
        // The classification head (spec FR-008): without it a cross-encoder produces
        // embeddings that look like relevance scores, and must be refused rather than used.
        required_tensors: &PINNED_Q8.classifier_tensors,
        quantisation: PINNED_Q8.quantisation,
        quantised_tensors: PINNED_Q8.quantised_tensors,
    }
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
