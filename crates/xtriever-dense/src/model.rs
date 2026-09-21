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
    /// What `bert.context_length` must say: at least the window the stage feeds.
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
/// The arithmetic the eight-bit matrices are multiplied in (`quantised_bert`): expanded to
/// `f16` at load, multiplied by the float kernel — the owner's choice over candle's eight-bit
/// CPU kernel, which is 3.7× slower for the sequences this stage feeds. A different arithmetic
/// would be different numbers, so the fingerprint names it.
macro_rules! compute {
    () => {
        "f16"
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
/// the artefact and `dtype=q8_0` in place of the float file, plus `compute=f16`, the arithmetic
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
/// architecture, block count, embedding length, head count, feed-forward length, context
/// length and the number of eight-bit tensors. The bytes were verified by [`verify_files_q8`];
/// this guards the pin itself — a pinned file that is not the model the forward pass is written
/// for is refused by name, not run.
///
/// # Errors
///
/// `Error::Model` naming the field and both values.
pub fn assert_gguf_header(bytes: &[u8]) -> xtriever_core::Result<()> {
    let header = GgufHeader::read(bytes)?;
    let architecture = header.string("general.architecture")?;
    if architecture != PINNED_Q8.architecture {
        return Err(model_err(format!(
            "GGUF header architecture is {architecture:?}, expected {:?}",
            PINNED_Q8.architecture
        )));
    }
    let prefix = PINNED_Q8.architecture;
    header.expect_number(&format!("{prefix}.block_count"), PINNED_Q8.blocks)?;
    header.expect_number(
        &format!("{prefix}.embedding_length"),
        PINNED_Q8.embedding_length,
    )?;
    header.expect_number(&format!("{prefix}.attention.head_count"), PINNED_Q8.heads)?;
    header.expect_number(
        &format!("{prefix}.feed_forward_length"),
        PINNED_Q8.feed_forward_length,
    )?;
    header.expect_number(
        &format!("{prefix}.context_length"),
        PINNED_Q8.context_length,
    )?;
    let quantised = header.quantised_tensors();
    if quantised != PINNED_Q8.quantised_tensors {
        return Err(model_err(format!(
            "GGUF file holds {quantised} {} tensors, expected {} — not the pinned artefact",
            PINNED_Q8.quantisation, PINNED_Q8.quantised_tensors
        )));
    }
    Ok(())
}

/// What a GGUF header declares, read with the pinned engine's own reader.
struct GgufHeader {
    content: candle_core::quantized::gguf_file::Content,
}

impl GgufHeader {
    fn read(bytes: &[u8]) -> xtriever_core::Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes);
        let content = candle_core::quantized::gguf_file::Content::read(&mut cursor)
            .map_err(|e| model_err(format!("not a GGUF file this engine can read: {e}")))?;
        Ok(Self { content })
    }

    fn string(&self, key: &str) -> xtriever_core::Result<&str> {
        self.content
            .metadata
            .get(key)
            .ok_or_else(|| model_err(format!("GGUF header declares no {key}")))?
            .to_string()
            .map(String::as_str)
            .map_err(|e| model_err(format!("GGUF header {key}: {e}")))
    }

    fn number(&self, key: &str) -> xtriever_core::Result<usize> {
        use candle_core::quantized::gguf_file::Value;
        let value = self
            .content
            .metadata
            .get(key)
            .ok_or_else(|| model_err(format!("GGUF header declares no {key}")))?;
        let n: u64 = match value {
            Value::U8(n) => u64::from(*n),
            Value::U16(n) => u64::from(*n),
            Value::U32(n) => u64::from(*n),
            Value::U64(n) => *n,
            Value::I8(n) if *n >= 0 => *n as u64,
            Value::I16(n) if *n >= 0 => *n as u64,
            Value::I32(n) if *n >= 0 => *n as u64,
            Value::I64(n) if *n >= 0 => *n as u64,
            other => {
                return Err(model_err(format!(
                    "GGUF header {key} is {other:?}, not a non-negative integer"
                )));
            }
        };
        usize::try_from(n).map_err(|_| model_err(format!("GGUF header {key} = {n} does not fit")))
    }

    fn expect_number(&self, key: &str, expected: usize) -> xtriever_core::Result<()> {
        let actual = self.number(key)?;
        if actual == expected {
            Ok(())
        } else {
            let field = key.rsplit('.').next().unwrap_or(key);
            Err(model_err(format!(
                "GGUF header {field} is {actual}, expected {expected} ({key})"
            )))
        }
    }

    fn quantised_tensors(&self) -> usize {
        self.content
            .tensor_infos
            .values()
            .filter(|t| t.ggml_dtype == candle_core::quantized::GgmlDType::Q8_0)
            .count()
    }
}

/// The layer-norm epsilon the artefact declares, when it does (the pinned files do: 1e-12,
/// the float configuration's value); `None` to use the configuration's.
pub(crate) fn gguf_layer_norm_epsilon(bytes: &[u8]) -> Option<f64> {
    let header = GgufHeader::read(bytes).ok()?;
    header
        .content
        .metadata
        .get("bert.attention.layer_norm_epsilon")
        .and_then(|v| v.to_f32().ok())
        .map(f64::from)
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
