//! The pinned `all-MiniLM-L6-v2` embedder on candle 0.9.2 (spec FR-001–FR-008; research D2–D6).
//!
//! Determinism is engineered, not inherited: every text goes through the model **alone**, padded
//! to a fixed 256 tokens, so its tensor shapes never depend on what else is in the caller's batch
//! (candle folds a batch into the matmul's `M`, research D2). The thread count is candle's
//! (`RAYON_NUM_THREADS`) and is recorded, never set.

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::Tokenizer;
use tokenizers::utils::padding::{PaddingParams, PaddingStrategy};
use tokenizers::utils::truncation::TruncationParams;
use xtriever_core::{Embedder, Metric, Result, TextKind, Vector};

use crate::error::model_err;
use crate::model::{FINGERPRINT, FINGERPRINT_Q8, PINNED, PINNED_Q8, Precision};
use crate::quantised_bert::{QuantisedBert, Shape};
use crate::{LoadPath, bytes};
use candle_transformers::quantized_var_builder::VarBuilder as QuantisedVarBuilder;

/// The encoder behind the embedder: candle's float BERT, or this crate's over the eight-bit
/// artefact. The pooling, normalisation and everything else are shared.
enum Encoder {
    Float(BertModel),
    EightBit(QuantisedBert),
}

impl Encoder {
    fn forward(
        &self,
        ids: &Tensor,
        type_ids: &Tensor,
        mask: Option<&Tensor>,
    ) -> candle_core::Result<Tensor> {
        match self {
            Self::Float(model) => model.forward(ids, type_ids, mask),
            Self::EightBit(model) => model.forward(ids, type_ids, mask),
        }
    }
}

/// The pinned `all-MiniLM-L6-v2` embedder, from either pinned artefact (Feature 026).
pub struct MiniLmEmbedder {
    tokenizer: Tokenizer,
    /// The same `tokenizer.json` without truncation or padding — answers "how many positions
    /// would this text need?" (Feature 008 D7). The embedding path never uses it.
    counter: Tokenizer,
    model: Encoder,
    device: Device,
    load_path: LoadPath,
    /// Which artefact the weights came from, and the fingerprint that names it.
    precision: Precision,
    fingerprint: &'static str,
}

impl std::fmt::Debug for MiniLmEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniLmEmbedder")
            .field("fingerprint", &self.fingerprint)
            .field("precision", &self.precision)
            .field("load_path", &self.load_path)
            .finish_non_exhaustive()
    }
}

impl MiniLmEmbedder {
    /// Verify (FR-003), assert (FR-002) and build. `dir` holds one pinned artefact: the float
    /// files (`config.json`, `tokenizer.json`, `model.safetensors`) or the eight-bit ones
    /// (the same configuration and tokenizer beside the pinned GGUF) — which one is what the
    /// manifest the installation fetched decided (Feature 026, spec FR-012).
    ///
    /// Order: the directory's form (exactly one weights file) → every file's size and hash →
    /// `config.json` parsed and asserted → tokenizer built from the verified bytes → the
    /// weights' header asserted → weights loaded through `load_path` → model built. Nothing is
    /// parsed before its bytes are verified.
    ///
    /// # Errors
    ///
    /// `Error::Model` for any verification, assertion or construction failure, naming the file
    /// and both values where a pin is violated; a directory holding both weights files, or
    /// neither, is refused naming both.
    pub fn load(dir: &Path, load_path: LoadPath) -> Result<Self> {
        let float = PINNED.files[2].name;
        let eight_bit = PINNED_Q8.files[2].name;
        match (dir.join(float).is_file(), dir.join(eight_bit).is_file()) {
            (true, true) => Err(model_err(format!(
                "{} holds both {float} and {eight_bit}; a model directory holds one pinned \
                 artefact, the one its manifest names",
                dir.display()
            ))),
            (false, false) => Err(model_err(format!(
                "{} holds neither {float} nor {eight_bit}; fetch one with scripts/fetch-model.sh",
                dir.display()
            ))),
            (true, false) => Self::load_float(dir, load_path),
            (false, true) => Self::load_eight_bit(dir, load_path),
        }
    }

    fn load_float(dir: &Path, load_path: LoadPath) -> Result<Self> {
        crate::model::verify_files(dir)?;

        let config = load_config(dir)?;
        let tokenizer = load_tokenizer(dir)?;
        let counter = load_counter(dir)?;

        let weights_path = dir.join(PINNED.files[2].name);
        let weights = bytes::read(&weights_path, load_path)?;
        assert_weights_header(weights.as_slice())?;

        let device = Device::Cpu;
        // Root prefix is empty: the safetensors keys carry no `bert.` prefix, and `BertModel::load`
        // applies `embeddings`/`encoder` itself.
        let vb = VarBuilder::from_slice_safetensors(weights.as_slice(), DTYPE, &device)
            .map_err(|e| model_err(format!("cannot load weights: {e}")))?;
        let model = BertModel::load(vb, &config)
            .map_err(|e| model_err(format!("cannot build model: {e}")))?;
        // `weights` is dropped here: candle copied every tensor into its own storage (D1).

        Ok(Self {
            tokenizer,
            counter,
            model: Encoder::Float(model),
            device,
            load_path,
            precision: Precision::Float,
            fingerprint: FINGERPRINT,
        })
    }

    /// The eight-bit artefact (Feature 026, research D5): the same configuration and tokenizer
    /// as the float path, the GGUF verified by checksum and then its header against the pin,
    /// and the encoder built over its tensors. `load_path` reads the file the same two ways;
    /// candle copies every tensor into its own storage either way, as it does for the float
    /// weights.
    fn load_eight_bit(dir: &Path, load_path: LoadPath) -> Result<Self> {
        crate::model::verify_files_q8(dir)?;

        let config = load_config(dir)?;
        let tokenizer = load_tokenizer(dir)?;
        let counter = load_counter(dir)?;

        let weights_path = dir.join(PINNED_Q8.files[2].name);
        let weights = bytes::read(&weights_path, load_path)?;
        let header = crate::model::checked_gguf_header(weights.as_slice())?;
        // The pinned configuration is the one source of the layer-norm epsilon; a file that
        // declares a different one is refused naming both, like every other pinned field.
        header.assert_layer_norm_epsilon(PINNED_Q8.architecture, config.layer_norm_eps)?;

        let device = Device::Cpu;
        let shape = Shape {
            vocabulary: config.vocab_size,
            blocks: PINNED_Q8.blocks,
            heads: PINNED_Q8.heads,
            hidden: PINNED_Q8.embedding_length,
            feed_forward: PINNED_Q8.feed_forward_length,
            context_length: PINNED_Q8.context_length,
            layer_norm_eps: config.layer_norm_eps,
        };
        // Every tensor is read once, here; the header above was the only other pass.
        let vb = QuantisedVarBuilder::from_gguf_buffer(weights.as_slice(), &device)
            .map_err(|e| model_err(format!("cannot read the eight-bit tensors: {e}")))?;
        let model = QuantisedBert::from_gguf(&vb, shape)
            .map_err(|e| model_err(format!("cannot build the eight-bit model: {e}")))?;

        Ok(Self {
            tokenizer,
            counter,
            model: Encoder::EightBit(model),
            device,
            load_path,
            precision: Precision::EightBit,
            fingerprint: FINGERPRINT_Q8,
        })
    }

    /// The path this instance was loaded through.
    #[must_use]
    pub fn load_path(&self) -> LoadPath {
        self.load_path
    }

    /// Which pinned artefact the weights came from (spec FR-012).
    #[must_use]
    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// candle's effective thread count (`RAYON_NUM_THREADS`, else the CPU count).
    #[must_use]
    pub fn thread_count() -> usize {
        candle_core::utils::get_num_threads()
    }

    /// Token ids and attention mask for `text`, for the tokenization-parity test.
    ///
    /// # Errors
    ///
    /// `Error::Model` if encoding fails.
    #[doc(hidden)]
    pub fn tokenize_for_test(&self, text: &str) -> Result<(Vec<u32>, Vec<u32>)> {
        let enc = self.encode(text)?;
        Ok((enc.get_ids().to_vec(), enc.get_attention_mask().to_vec()))
    }

    /// The number of positions `text` needs to be seen whole — `[CLS]`, its word-pieces,
    /// `[SEP]` — with **no truncation**: a text longer than the window counts past 256.
    ///
    /// The embedder itself truncates and pads to `max_tokens`, so this is the only way to ask
    /// whether a passage fits; Feature 008's chunker budgets on `token_count(unit) - 2`, the
    /// content pieces, which are additive over whitespace-joined units (the BERT pre-tokenizer
    /// splits on whitespace and punctuation before WordPiece runs per word). The embedding
    /// behaviour and `MODEL_ID` are unchanged by this method.
    ///
    /// # Errors
    ///
    /// `Error::Model` if encoding fails.
    pub fn token_count(&self, text: &str) -> Result<usize> {
        self.counter
            .encode(text, true)
            .map(|enc| enc.get_ids().len())
            .map_err(|e| model_err(format!("cannot encode text: {e}")))
    }

    fn encode(&self, text: &str) -> Result<tokenizers::Encoding> {
        self.tokenizer
            .encode(text, true)
            .map_err(|e| model_err(format!("cannot encode text: {e}")))
    }

    /// One text through the model: batch dimension 1, sequence length exactly `max_tokens`.
    fn embed_one(&self, text: &str) -> Result<Vector> {
        let enc = self.encode(text)?;
        let seq = enc.get_ids().len();
        if seq != PINNED.max_tokens {
            return Err(model_err(format!(
                "tokenizer produced {seq} positions, expected {}",
                PINNED.max_tokens
            )));
        }
        let ids = self.tensor(enc.get_ids(), seq, "input_ids")?;
        let type_ids = self.tensor(enc.get_type_ids(), seq, "token_type_ids")?;
        let mask = self.tensor(enc.get_attention_mask(), seq, "attention_mask")?;

        // `token_type_ids` is a required positional argument in candle 0.9.2, not an Option.
        let hidden = self
            .model
            .forward(&ids, &type_ids, Some(&mask))
            .map_err(|e| model_err(format!("forward pass: {e}")))?;

        // Attention-mask-weighted mean pooling, then L2 normalisation — the sentence-transformers
        // reference pipeline and Feature 001's exact expressions. Padding is excluded by the
        // mask weighting.
        let mask_f = mask
            .to_dtype(DType::F32)
            .and_then(|m| m.unsqueeze(2))
            .map_err(|e| model_err(format!("mask dtype: {e}")))?;
        let summed = hidden
            .broadcast_mul(&mask_f)
            .and_then(|t| t.sum(1))
            .map_err(|e| model_err(format!("masked sum: {e}")))?;
        let counts = mask_f
            .sum(1)
            .map_err(|e| model_err(format!("mask sum: {e}")))?;
        let pooled = summed
            .broadcast_div(&counts)
            .map_err(|e| model_err(format!("mean pool: {e}")))?;
        let normalized = pooled
            .sqr()
            .and_then(|t| t.sum_keepdim(1))
            .and_then(|t| t.sqrt())
            .and_then(|norm| pooled.broadcast_div(&norm))
            .map_err(|e| model_err(format!("l2 normalize: {e}")))?;

        let mut rows: Vec<Vec<f32>> = normalized
            .to_vec2()
            .map_err(|e| model_err(format!("read embedding: {e}")))?;
        let vector = rows
            .pop()
            .ok_or_else(|| model_err("forward pass produced no rows"))?;
        if vector.len() != PINNED.dim {
            return Err(model_err(format!(
                "expected {} dimensions, got {}",
                PINNED.dim,
                vector.len()
            )));
        }
        Ok(vector)
    }

    fn tensor(&self, data: &[u32], seq: usize, what: &str) -> Result<Tensor> {
        Tensor::new(data, &self.device)
            .and_then(|t| t.reshape((1, seq)))
            .map_err(|e| model_err(format!("{what} tensor: {e}")))
    }
}

impl Embedder for MiniLmEmbedder {
    fn dim(&self) -> usize {
        PINNED.dim
    }

    fn metric(&self) -> Metric {
        Metric::Cosine
    }

    fn fingerprint(&self) -> &str {
        self.fingerprint
    }

    fn max_input_tokens(&self) -> Option<usize> {
        Some(PINNED.max_tokens)
    }

    /// One vector per text, in order. `kind` is accepted and ignored: this model is symmetric
    /// and applies no prefixes, which the fingerprint records as `prefix=none` (FR-007).
    fn embed(&self, texts: &[&str], _kind: TextKind) -> Result<Vec<Vector>> {
        texts.iter().map(|text| self.embed_one(text)).collect()
    }
}

/// Parse and assert `config.json` (FR-002). The bytes were verified by `verify_files`.
fn load_config(dir: &Path) -> Result<Config> {
    let path = dir.join(PINNED.files[0].name);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| model_err(format!("cannot read {}: {e}", path.display())))?;
    let config: Config = serde_json::from_str(&text)
        .map_err(|e| model_err(format!("cannot parse {}: {e}", path.display())))?;
    let raw: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| model_err(format!("cannot parse {}: {e}", path.display())))?;

    let assert = |name: &str, actual: usize, expected: usize| {
        if actual == expected {
            Ok(())
        } else {
            Err(model_err(format!(
                "config.json {name} is {actual}, expected {expected}"
            )))
        }
    };
    assert("hidden_size", config.hidden_size, PINNED.dim)?;
    assert("vocab_size", config.vocab_size, 30_522)?;
    assert("pad_token_id", config.pad_token_id, 0)?;
    if config.max_position_embeddings < PINNED.max_tokens {
        return Err(model_err(format!(
            "config.json max_position_embeddings is {}, below the {} tokens this embedder feeds",
            config.max_position_embeddings, PINNED.max_tokens
        )));
    }
    match raw.get("model_type").and_then(serde_json::Value::as_str) {
        Some("bert") => {}
        other => {
            return Err(model_err(format!(
                "config.json model_type is {other:?}, expected \"bert\""
            )));
        }
    }
    Ok(config)
}

/// The counting tokenizer: the verified `tokenizer.json` with truncation and padding removed
/// (`Tokenizer::with_truncation(None)`, `with_padding(None)`), so an encoding's length is the
/// text's true position count (Feature 008 D7).
fn load_counter(dir: &Path) -> Result<Tokenizer> {
    let path = dir.join(PINNED.files[1].name);
    let bytes = std::fs::read(&path)
        .map_err(|e| model_err(format!("cannot read {}: {e}", path.display())))?;
    let mut tokenizer = Tokenizer::from_bytes(&bytes)
        .map_err(|e| model_err(format!("cannot load {}: {e}", path.display())))?;
    tokenizer
        .with_truncation(None)
        .map_err(|e| model_err(format!("cannot clear truncation: {e}")))?;
    tokenizer.with_padding(None);
    Ok(tokenizer)
}

/// Build the tokenizer from the verified bytes with the sentence-transformers overrides:
/// truncation and fixed padding at `max_tokens` (`tokenizer.json` itself ships 128 — 001 D6).
fn load_tokenizer(dir: &Path) -> Result<Tokenizer> {
    let path = dir.join(PINNED.files[1].name);
    let bytes = std::fs::read(&path)
        .map_err(|e| model_err(format!("cannot read {}: {e}", path.display())))?;
    let mut tokenizer = Tokenizer::from_bytes(&bytes)
        .map_err(|e| model_err(format!("cannot load {}: {e}", path.display())))?;
    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: PINNED.max_tokens,
            ..TruncationParams::default()
        }))
        .map_err(|e| model_err(format!("cannot set truncation: {e}")))?;
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(PINNED.max_tokens),
        pad_id: 0,
        pad_token: "[PAD]".to_owned(),
        ..PaddingParams::default()
    }));
    match tokenizer.token_to_id("[PAD]") {
        Some(0) => Ok(tokenizer),
        other => Err(model_err(format!(
            "tokenizer.json maps [PAD] to {other:?}, expected 0"
        ))),
    }
}

/// Assert the safetensors header: exactly `f32_tensors` `F32` tensors and only
/// `embeddings.position_ids` as `I64` (001 FR-033 — a quantised variant differs here).
fn assert_weights_header(bytes: &[u8]) -> Result<()> {
    let name = PINNED.files[2].name;
    let Some(len_bytes) = bytes.get(..8) else {
        return Err(model_err(format!(
            "{name}: shorter than a safetensors header"
        )));
    };
    let header_len = usize::try_from(u64::from_le_bytes(
        len_bytes
            .try_into()
            .map_err(|_| model_err(format!("{name}: bad header length")))?,
    ))
    .map_err(|e| model_err(format!("{name}: implausible header length: {e}")))?;
    let Some(header_bytes) = bytes.get(8..8 + header_len) else {
        return Err(model_err(format!(
            "{name}: header length {header_len} exceeds the file"
        )));
    };
    let header: serde_json::Value = serde_json::from_slice(header_bytes)
        .map_err(|e| model_err(format!("{name}: header is not JSON: {e}")))?;
    let tensors = header
        .as_object()
        .ok_or_else(|| model_err(format!("{name}: header is not an object")))?;
    let mut f32_tensors = 0usize;
    for (tensor, meta) in tensors {
        if tensor == "__metadata__" {
            continue;
        }
        match meta.get("dtype").and_then(serde_json::Value::as_str) {
            Some("F32") => f32_tensors += 1,
            // An integer index buffer, not a weight; `I64` in the published fp32 model.
            Some("I64") if tensor == "embeddings.position_ids" => {}
            other => {
                return Err(model_err(format!(
                    "{name}: tensor {tensor} has dtype {other:?}; the pin is as-published f32 weights"
                )));
            }
        }
    }
    if f32_tensors != PINNED.f32_tensors {
        return Err(model_err(format!(
            "{name}: {f32_tensors} F32 tensors, expected {} — not the pinned fp32 model",
            PINNED.f32_tensors
        )));
    }
    Ok(())
}
