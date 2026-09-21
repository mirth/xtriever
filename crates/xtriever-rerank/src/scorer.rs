//! The pinned `ms-marco-MiniLM-L-6-v2` cross-encoder on candle 0.9.2 (spec FR-001–FR-006;
//! research D1, D4, D5).
//!
//! Each query–passage pair goes through the model **alone, at its own length**: a pair's tensor
//! shapes depend only on the pair, so its score cannot depend on the other passages in the
//! call, their order or the call boundaries (candle folds a batch into the matmul's `M`, 004
//! research D2 — the reason nothing is batched here). candle-transformers 0.9.2 ships the BERT
//! encoder without a classification head; the head — CLS row → pooler → tanh → linear — is the
//! reference's, composed from two `candle_nn::Linear` layers (research D1).

use std::path::Path;

use candle_core::{Device, Module, Tensor};
use candle_nn::{Linear, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::Tokenizer;
use tokenizers::utils::truncation::TruncationParams;
use xtriever_core::{Budget, Passage, Reranker, Result};

use crate::error::model_err;
use crate::model::{MODEL_ID, MODEL_ID_Q8, PINNED, PINNED_Q8, Precision};
use crate::quantised_bert::{QuantisedBert, Shape};
use crate::{LoadPath, bytes};
use candle_transformers::quantized_var_builder::VarBuilder as QuantisedVarBuilder;

/// The encoder behind the cross-encoder: candle's float BERT, or this crate's over the
/// eight-bit artefact. The head — pooler, tanh, classifier — is the same float arithmetic
/// either way.
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

/// The pinned `ms-marco-MiniLM-L-6-v2` cross-encoder, from either pinned artefact (Feature 026).
pub struct MiniLmCrossEncoder {
    tokenizer: Tokenizer,
    encoder: Encoder,
    pooler: Linear,
    classifier: Linear,
    device: Device,
    load_path: LoadPath,
    /// Which artefact the weights came from, and the identity string that names it.
    precision: Precision,
    model_id: &'static str,
}

impl std::fmt::Debug for MiniLmCrossEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniLmCrossEncoder")
            .field("model_id", &self.model_id)
            .field("precision", &self.precision)
            .field("load_path", &self.load_path)
            .finish_non_exhaustive()
    }
}

impl MiniLmCrossEncoder {
    /// Verify (FR-003), assert (FR-002) and build. `dir` holds one pinned artefact: the float
    /// files (`config.json`, `tokenizer.json`, `model.safetensors`) or the eight-bit ones (the
    /// same configuration and tokenizer beside the pinned GGUF, and `pooler.safetensors`, the
    /// float model's pooler cut byte for byte) — which one is what the manifest the installation
    /// fetched decided (Feature 026, spec FR-012).
    ///
    /// Order: the directory's form (exactly one weights file) → every file's size and hash →
    /// `config.json` parsed and asserted → tokenizer built from the verified bytes → the
    /// weights' header asserted (safetensors, or GGUF against the pin including the
    /// classification head) → weights read through `load_path` → encoder, pooler and
    /// classifier built. Nothing is parsed before its bytes are verified.
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

        let weights_path = dir.join(PINNED.files[2].name);
        let weights = bytes::read(&weights_path, load_path)
            .map_err(|e| model_err(format!("cannot read {}: {e}", weights_path.display())))?;
        assert_weights_header(weights.as_slice())?;

        let device = Device::Cpu;
        let vb = VarBuilder::from_slice_safetensors(weights.as_slice(), DTYPE, &device)
            .map_err(|e| model_err(format!("cannot load weights: {e}")))?;
        // The safetensors keys carry the `bert.` prefix; the head sits beside it (research D2).
        let encoder = BertModel::load(vb.pp("bert"), &config)
            .map_err(|e| model_err(format!("cannot build encoder: {e}")))?;
        let pooler = candle_nn::linear(PINNED.hidden, PINNED.hidden, vb.pp("bert.pooler.dense"))
            .map_err(|e| model_err(format!("cannot build pooler: {e}")))?;
        let classifier = candle_nn::linear(PINNED.hidden, 1, vb.pp("classifier"))
            .map_err(|e| model_err(format!("cannot build classifier: {e}")))?;
        // `weights` is dropped here: candle copied every tensor into its own storage.

        Ok(Self {
            tokenizer,
            encoder: Encoder::Float(encoder),
            pooler,
            classifier,
            device,
            load_path,
            precision: Precision::Float,
            model_id: MODEL_ID,
        })
    }

    /// The eight-bit artefact (Feature 026, research D5): the same configuration and tokenizer
    /// as the float path; the GGUF verified by checksum, its header asserted against the pin —
    /// including the classification head's presence (FR-008) — and the encoder built over its
    /// tensors; the classifier taken from the GGUF (float in the file, the published head bit
    /// for bit); and the pooler, which the artefact lacks, from `pooler.safetensors`, the pinned
    /// float model's own two tensors (owner's decision, 2026-09-21).
    fn load_eight_bit(dir: &Path, load_path: LoadPath) -> Result<Self> {
        crate::model::verify_files_q8(dir)?;

        let config = load_config(dir)?;
        let tokenizer = load_tokenizer(dir)?;

        let weights_path = dir.join(PINNED_Q8.files[2].name);
        let weights = bytes::read(&weights_path, load_path)
            .map_err(|e| model_err(format!("cannot read {}: {e}", weights_path.display())))?;
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
        // Every tensor is read once, here, for the encoder and the classifier alike.
        let vb = QuantisedVarBuilder::from_gguf_buffer(weights.as_slice(), &device)
            .map_err(|e| model_err(format!("cannot read the eight-bit tensors: {e}")))?;
        let encoder = QuantisedBert::from_gguf(&vb, shape)
            .map_err(|e| model_err(format!("cannot build the eight-bit encoder: {e}")))?;
        let classifier = classifier_from_gguf(&vb, &device)?;

        let pooler_path = dir.join(PINNED_Q8.files[3].name);
        let pooler_bytes = bytes::read(&pooler_path, load_path)
            .map_err(|e| model_err(format!("cannot read {}: {e}", pooler_path.display())))?;
        let vb = VarBuilder::from_slice_safetensors(pooler_bytes.as_slice(), DTYPE, &device)
            .map_err(|e| model_err(format!("cannot load the pooler: {e}")))?;
        let pooler = candle_nn::linear(PINNED.hidden, PINNED.hidden, vb.pp("bert.pooler.dense"))
            .map_err(|e| model_err(format!("cannot build pooler: {e}")))?;

        Ok(Self {
            tokenizer,
            encoder: Encoder::EightBit(encoder),
            pooler,
            classifier,
            device,
            load_path,
            precision: Precision::EightBit,
            model_id: MODEL_ID_Q8,
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

    /// Token ids and token type ids for the pair, for the tokenization-parity test.
    ///
    /// # Errors
    ///
    /// `Error::Model` if encoding fails.
    #[doc(hidden)]
    pub fn tokenize_for_test(&self, query: &str, passage: &str) -> Result<(Vec<u32>, Vec<u32>)> {
        let enc = self.encode(query, passage)?;
        Ok((enc.get_ids().to_vec(), enc.get_type_ids().to_vec()))
    }

    /// The reference's call, literally: `tokenizer(query, passage, truncation=True,
    /// max_length=512)` — and an empty passage is falsy there, so the query is encoded alone
    /// (research D4).
    fn encode(&self, query: &str, passage: &str) -> Result<tokenizers::Encoding> {
        let result = if passage.is_empty() {
            self.tokenizer.encode(query, true)
        } else {
            self.tokenizer.encode((query, passage), true)
        };
        result.map_err(|e| model_err(format!("cannot encode pair: {e}")))
    }

    /// Score one query–passage pair: one forward pass at the pair's own length, then the
    /// classification head on the CLS row.
    ///
    /// # Errors
    ///
    /// `Error::Model` if encoding or the forward pass fails, or the logit is not finite (FR-008).
    pub fn score(&self, query: &str, passage: &str) -> Result<f32> {
        let enc = self.encode(query, passage)?;
        let seq = enc.get_ids().len();
        let ids = self.tensor(enc.get_ids(), seq, "input_ids")?;
        let type_ids = self.tensor(enc.get_type_ids(), seq, "token_type_ids")?;

        // No padding ⇒ the attention mask is all ones, which `None` means in candle 0.9.2.
        let hidden = self
            .encoder
            .forward(&ids, &type_ids, None)
            .map_err(|e| model_err(format!("forward pass: {e}")))?;
        let cls = hidden
            .narrow(1, 0, 1)
            .and_then(|t| t.squeeze(1))
            .map_err(|e| model_err(format!("cls row: {e}")))?;
        let pooled = self
            .pooler
            .forward(&cls)
            .and_then(|t| t.tanh())
            .map_err(|e| model_err(format!("pooler: {e}")))?;
        let logit: f32 = self
            .classifier
            .forward(&pooled)
            .and_then(|t| t.squeeze(1))
            .and_then(|t| t.squeeze(0))
            .and_then(|t| t.to_scalar::<f32>())
            .map_err(|e| model_err(format!("classifier: {e}")))?;
        if !logit.is_finite() {
            return Err(model_err(format!(
                "model produced a non-finite score {logit}"
            )));
        }
        Ok(logit)
    }

    fn tensor(&self, data: &[u32], seq: usize, what: &str) -> Result<Tensor> {
        Tensor::new(data, &self.device)
            .and_then(|t| t.reshape((1, seq)))
            .map_err(|e| model_err(format!("{what} tensor: {e}")))
    }
}

impl Reranker for MiniLmCrossEncoder {
    fn model_id(&self) -> &str {
        self.model_id
    }

    fn rerank(
        &self,
        query: &str,
        passages: &[Passage<'_>],
        budget: &Budget,
    ) -> Result<Vec<Option<f32>>> {
        crate::budget::rerank_with(&|q, p| self.score(q, p), query, passages, budget)
    }
}

/// The classification head from the eight-bit artefact: `classifier.weight` (stored flat, 384)
/// and `classifier.bias` (1), float in the file, reshaped to the `(1, hidden)` linear the float
/// path builds. The header check already required both tensors.
fn classifier_from_gguf(vb: &QuantisedVarBuilder, device: &Device) -> Result<Linear> {
    let weight = vb
        .get_no_shape("classifier.weight")
        .and_then(|t| t.dequantize(device))
        .and_then(|t| t.reshape((1, PINNED.hidden)))
        .map_err(|e| model_err(format!("classifier.weight: {e}")))?;
    let bias = vb
        .get_no_shape("classifier.bias")
        .and_then(|t| t.dequantize(device))
        .map_err(|e| model_err(format!("classifier.bias: {e}")))?;
    Ok(Linear::new(weight, Some(bias)))
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
    assert("hidden_size", config.hidden_size, PINNED.hidden)?;
    assert("vocab_size", config.vocab_size, 30_522)?;
    assert("pad_token_id", config.pad_token_id, 0)?;
    if config.max_position_embeddings < PINNED.max_tokens {
        return Err(model_err(format!(
            "config.json max_position_embeddings is {}, below the {} tokens a pair may have",
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
    let architectures = raw
        .get("architectures")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !architectures.contains(&"BertForSequenceClassification") {
        return Err(model_err(format!(
            "config.json architectures is {architectures:?}, expected BertForSequenceClassification"
        )));
    }
    let labels = raw
        .get("id2label")
        .and_then(serde_json::Value::as_object)
        .map_or(0, serde_json::Map::len);
    if labels != 1 {
        return Err(model_err(format!(
            "config.json id2label has {labels} entries, expected exactly 1 (a single relevance logit)"
        )));
    }
    Ok(config)
}

/// Build the tokenizer from the verified bytes with the reference's truncation: `longest_first`
/// at 512 (the `tokenizers` default strategy), no padding (research D4).
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
    tokenizer.with_padding(None);
    for (token, expected) in [("[PAD]", 0), ("[CLS]", 101), ("[SEP]", 102)] {
        match tokenizer.token_to_id(token) {
            Some(id) if id == expected => {}
            other => {
                return Err(model_err(format!(
                    "tokenizer.json maps {token} to {other:?}, expected {expected}"
                )));
            }
        }
    }
    Ok(tokenizer)
}

/// Assert the safetensors header: exactly `f32_tensors` `F32` tensors, only
/// `bert.embeddings.position_ids` as `I64`, and the classifier's shape (research D2 — a
/// quantised variant or a model without the head differs here).
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
            Some("I64") if tensor == "bert.embeddings.position_ids" => {}
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
    let classifier_shape: Vec<u64> = tensors
        .get("classifier.weight")
        .and_then(|m| m.get("shape"))
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(serde_json::Value::as_u64).collect())
        .unwrap_or_default();
    if classifier_shape != [1, PINNED.hidden as u64] {
        return Err(model_err(format!(
            "{name}: classifier.weight has shape {classifier_shape:?}, expected [1, {}]",
            PINNED.hidden
        )));
    }
    Ok(())
}
