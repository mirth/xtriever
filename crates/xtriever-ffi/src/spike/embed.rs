//! `spike_embed` — embed one sentence with all-MiniLM-L6-v2.

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::Tokenizer;
use tokenizers::utils::padding::{PaddingParams, PaddingStrategy};
use tokenizers::utils::truncation::TruncationParams;

use crate::ffi::{LoadPath, SpikeError};

/// Sequence length the reference pipeline uses.
///
/// The model's `tokenizer.json` bakes in **128**; `sentence_bert_config.json` and the model card
/// say **256**. Python's `AutoTokenizer` overrides the former from config, a plain
/// `Tokenizer::from_file` does not — so without this override Rust and Python disagree with no iOS
/// involvement whatsoever (research D6). That failure would look exactly like an embedding bug,
/// which is why `tests/tokenize.rs` checks the token sequence *before* `tests/embed.rs` compares
/// vectors.
pub const MAX_SEQUENCE_LENGTH: usize = 256;

/// Expected output dimensionality.
pub const EMBEDDING_DIM: usize = 384;

/// Exact size of the pinned `model.safetensors` (FR-016).
const WEIGHTS_BYTES: u64 = 90_868_376;

/// A tokenized sentence, padded and truncated to the model's sequence length.
///
/// Internal to the crate: not a uniffi type and not exposed to Swift. It exists so the
/// tokenization-parity oracle can be checked on its own, *before* the embedding comparison —
/// which is what stops a tokenizer disagreement being misfiled as an iOS embedding failure
/// (research D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokenized {
    /// Token ids, length [`MAX_SEQUENCE_LENGTH`].
    pub input_ids: Vec<u32>,
    /// 1 for real tokens, 0 for padding.
    pub attention_mask: Vec<u32>,
    /// All zero for a single-segment sentence; required positionally by candle's `forward`.
    pub token_type_ids: Vec<u32>,
}

fn model_err(message: impl std::fmt::Display) -> SpikeError {
    SpikeError::Model {
        message: message.to_string(),
    }
}

fn tokenize_err(message: impl std::fmt::Display) -> SpikeError {
    SpikeError::Tokenize {
        message: message.to_string(),
    }
}

fn inference_err(message: impl std::fmt::Display) -> SpikeError {
    SpikeError::Inference {
        message: message.to_string(),
    }
}

/// The thread count candle will actually use, for the harness to record alongside measurements.
///
/// Reads `RAYON_NUM_THREADS`, falling back to the host CPU count. See [`run`] for why the caller,
/// not this crate, is responsible for pinning it.
#[must_use]
pub fn thread_count() -> usize {
    candle_core::utils::get_num_threads()
}

/// Tokenize `sentence` with the model's tokenizer, overriding truncation and padding to 256.
///
/// # Errors
///
/// [`SpikeError::Tokenize`] if `tokenizer.json` cannot be read or the sentence cannot be encoded.
pub fn tokenize(model_dir: &str, sentence: &str) -> Result<Tokenized, SpikeError> {
    let path = Path::new(model_dir).join("tokenizer.json");
    let mut tokenizer = Tokenizer::from_file(&path)
        .map_err(|e| tokenize_err(format!("cannot load {}: {e}", path.display())))?;

    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: MAX_SEQUENCE_LENGTH,
            ..TruncationParams::default()
        }))
        .map_err(|e| tokenize_err(format!("cannot set truncation: {e}")))?;
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(MAX_SEQUENCE_LENGTH),
        pad_id: 0,
        pad_token: "[PAD]".to_owned(),
        ..PaddingParams::default()
    }));

    let encoding = tokenizer
        .encode(sentence, true)
        .map_err(|e| tokenize_err(format!("cannot encode: {e}")))?;

    Ok(Tokenized {
        input_ids: encoding.get_ids().to_vec(),
        attention_mask: encoding.get_attention_mask().to_vec(),
        token_type_ids: encoding.get_type_ids().to_vec(),
    })
}

/// Verify the weights are byte-for-byte the artifact this spec pinned, then build a `VarBuilder`.
///
/// The size check is a hard error rather than a warning (FR-016): a different file means a
/// different golden, and every comparison downstream would silently inherit it.
fn var_builder<'a>(
    model_dir: &str,
    load_path: LoadPath,
    device: &Device,
) -> Result<VarBuilder<'a>, SpikeError> {
    let weights = Path::new(model_dir).join("model.safetensors");
    let size = std::fs::metadata(&weights)
        .map_err(|e| model_err(format!("cannot stat {}: {e}", weights.display())))?
        .len();
    if size != WEIGHTS_BYTES {
        return Err(model_err(format!(
            "{} is {size} bytes, expected exactly {WEIGHTS_BYTES}",
            weights.display()
        )));
    }

    match load_path {
        LoadPath::Buffered => {
            let data = std::fs::read(&weights)
                .map_err(|e| model_err(format!("cannot read {}: {e}", weights.display())))?;
            VarBuilder::from_buffered_safetensors(data, DTYPE, device)
                .map_err(|e| model_err(format!("cannot load weights: {e}")))
        }
        LoadPath::Mmapped => mmapped_var_builder(&weights, device),
    }
}

/// Memory-map the weights instead of reading them onto the heap.
///
/// This is the single `unsafe` exemption granted by
/// [ADR-0002](../../../../docs/adr/0002-unsafe-mmap-safetensors-measurement.md). It exists to
/// measure whether mmap'd, file-backed pages stay out of `phys_footprint` — 87.1 MiB against a
/// 300 MB ceiling is the largest single lever on the spike's memory verdict, and it is not
/// knowable from documentation.
///
/// `tests/load_paths.rs` asserts this path agrees bit-for-bit with the safe one (ADR-0002
/// condition 4). If it ever disagrees, this function is deleted rather than accommodated.
#[allow(unsafe_code)]
fn mmapped_var_builder<'a>(weights: &Path, device: &Device) -> Result<VarBuilder<'a>, SpikeError> {
    // SAFETY: `from_mmaped_safetensors` maps the file and hands out references into that mapping,
    // so the caller must guarantee the file is neither modified nor truncated for the lifetime of
    // the returned `VarBuilder`. That holds here: the weights are a read-only resource inside the
    // signed application bundle (or, on the host, a fixture verified by exact byte count and hash
    // immediately above), nothing in this process writes to them, and the mapping is confined to
    // this call's return value.
    let builder = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], DTYPE, device) };
    builder.map_err(|e| model_err(format!("cannot mmap weights: {e}")))
}

/// Embed `sentence`, returning [`EMBEDDING_DIM`] L2-normalized floats.
///
/// # Thread count — the caller's responsibility
///
/// candle's CPU backend uses rayon unconditionally and sizes its pool from the host core count,
/// which perturbs both the memory footprint this spike measures and float summation order
/// (research D15). candle 0.9.2 offers only a getter, `candle_core::utils::get_num_threads`, which
/// reads **`RAYON_NUM_THREADS`** — note *not* `CANDLE_NUM_THREADS`, which is what research D15
/// recorded and what later candle versions read. There is no setter, and a library has no business
/// mutating process-global environment anyway (in edition 2024 `std::env::set_var` is `unsafe`,
/// and ADR-0002 grants exactly one `unsafe` block, spent on the mmap loader).
///
/// So the **caller** must export `RAYON_NUM_THREADS=1` before the process starts. Use
/// [`thread_count`] to record what was actually in effect.
///
/// # Errors
///
/// [`SpikeError::Model`] if an artifact is missing or fails verification, [`SpikeError::Tokenize`]
/// if encoding fails, [`SpikeError::Inference`] if the forward pass fails.
pub fn run(model_dir: &str, sentence: &str, load_path: LoadPath) -> Result<Vec<f32>, SpikeError> {
    let tokens = tokenize(model_dir, sentence)?;
    let device = Device::Cpu;

    let config_path = Path::new(model_dir).join("config.json");
    let config_text = std::fs::read_to_string(&config_path)
        .map_err(|e| model_err(format!("cannot read {}: {e}", config_path.display())))?;
    let config: Config = serde_json::from_str(&config_text)
        .map_err(|e| model_err(format!("cannot parse {}: {e}", config_path.display())))?;

    let vb = var_builder(model_dir, load_path, &device)?;
    // Root prefix is empty: the safetensors keys carry no `bert.` prefix, and `BertModel::load`
    // applies `embeddings`/`encoder` itself.
    let model = BertModel::load(vb, &config)
        .map_err(|e| inference_err(format!("cannot build model: {e}")))?;

    let seq = tokens.input_ids.len();
    let ids = Tensor::new(tokens.input_ids.as_slice(), &device)
        .and_then(|t| t.reshape((1, seq)))
        .map_err(|e| inference_err(format!("input_ids tensor: {e}")))?;
    let type_ids = Tensor::new(tokens.token_type_ids.as_slice(), &device)
        .and_then(|t| t.reshape((1, seq)))
        .map_err(|e| inference_err(format!("token_type_ids tensor: {e}")))?;
    let mask = Tensor::new(tokens.attention_mask.as_slice(), &device)
        .and_then(|t| t.reshape((1, seq)))
        .map_err(|e| inference_err(format!("attention_mask tensor: {e}")))?;

    // `token_type_ids` is a required positional argument in candle 0.9.2, not an Option.
    let hidden = model
        .forward(&ids, &type_ids, Some(&mask))
        .map_err(|e| inference_err(format!("forward pass: {e}")))?;

    // Attention-mask-weighted mean pooling, then L2 normalization — the sentence-transformers
    // reference pipeline (`1_Pooling/config.json` + the `Normalize` module). Padding must not be
    // averaged in, which is exactly what weighting by the mask prevents.
    let pooled = {
        let mask_f = mask
            .to_dtype(DType::F32)
            .and_then(|m| m.unsqueeze(2))
            .map_err(|e| inference_err(format!("mask dtype: {e}")))?;
        let summed = hidden
            .broadcast_mul(&mask_f)
            .and_then(|t| t.sum(1))
            .map_err(|e| inference_err(format!("masked sum: {e}")))?;
        let counts = mask_f
            .sum(1)
            .map_err(|e| inference_err(format!("mask sum: {e}")))?;
        summed
            .broadcast_div(&counts)
            .map_err(|e| inference_err(format!("mean pool: {e}")))?
    };

    let normalized = pooled
        .sqr()
        .and_then(|t| t.sum_keepdim(1))
        .and_then(|t| t.sqrt())
        .and_then(|norm| pooled.broadcast_div(&norm))
        .map_err(|e| inference_err(format!("l2 normalize: {e}")))?;

    let mut rows: Vec<Vec<f32>> = normalized
        .to_vec2()
        .map_err(|e| inference_err(format!("read embedding: {e}")))?;
    let vector = rows
        .pop()
        .ok_or_else(|| inference_err("forward pass produced no rows"))?;

    if vector.len() != EMBEDDING_DIM {
        return Err(inference_err(format!(
            "expected {EMBEDDING_DIM} dimensions, got {}",
            vector.len()
        )));
    }
    Ok(vector)
}
