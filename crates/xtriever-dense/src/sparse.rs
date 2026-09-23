//! The sparse document encoder and its query side (Feature 027, research D1–D4, D7).
//!
//! **The encoder** ([`SparseEncoder`], build host only) is the pinned
//! `opensearch-neural-sparse-encoding-doc-v3-distill` ([`PINNED_SPARSE`]): a DistilBERT with its
//! masked-language-model head. A document's expansion is the head's logits, maximised over
//! positions per vocabulary entry, through `log1p(log1p(relu(·)))`, special tokens zeroed, entries
//! above zero kept — the recipe the Feature 012 spike measured (research D3). Each document is
//! encoded alone at its own length, so a weight never depends on what it was batched with
//! (Principle VI).
//!
//! The forward pass is written out here over `candle_nn`'s layers rather than taken from
//! `candle_transformers::models::distilbert`, because candle 0.9.2's DistilBERT maps the config's
//! `activation: "gelu"` to `Tensor::gelu` — the tanh approximation — where the model's reference
//! (`transformers`' `DistilBertForMaskedLM`) uses exact GELU, as candle's own BERT does
//! (`gelu_erf`). Tensor names and layer order follow `candle-transformers-0.9.2/src/models/
//! distilbert.rs` line for line; only the activation differs (research D2). The oracle,
//! `tests/sparse_oracle.rs`, holds every weight within 1e-4 of the reference.
//!
//! **The query side** ([`SparseQuery`]) needs no model: the query's distinct token ids, special
//! tokens excluded, kept if the query-side table (`idf.json`) gives them a positive entry
//! (research D7). A sparse index stores both of its files and opens them through here.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use candle_core::{D, DType, Device, Module, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, VarBuilder};
use tokenizers::{Tokenizer, TruncationParams};
use xtriever_core::Result;

use crate::LoadPath;
use crate::bytes;
use crate::error::{corrupt, sparse_err};
use crate::model::{PINNED_SPARSE, SPARSE_IDENTITY};

/// The configuration the forward pass is written for; `config.json` must say exactly this.
const DIM: usize = 768;
const LAYERS: usize = 6;
const HEADS: usize = 12;
const HIDDEN: usize = 3072;
const VOCABULARY: usize = 30_522;
/// DistilBERT's layer-norm epsilon, fixed in the architecture rather than the configuration.
const LAYER_NORM_EPS: f64 = 1e-12;

/// The tokens whose weights the recipe zeroes and the query side never keeps.
const SPECIAL_TOKENS: [&str; 5] = ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]"];

/// One document's expansion: its kept `(token id, weight)` entries, ascending by id, every
/// weight above zero, special tokens absent (research D3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expansion {
    /// `(token id, weight)`, ascending by id.
    pub entries: Vec<(u32, f32)>,
    /// The document ran past the encoder's window and was truncated to it.
    pub truncated: bool,
}

/// The pinned document encoder — build host only.
pub struct SparseEncoder {
    tokenizer: Tokenizer,
    special: Vec<u32>,
    embeddings: Embeddings,
    blocks: Vec<Block>,
    head: Head,
    device: Device,
}

impl std::fmt::Debug for SparseEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparseEncoder")
            .field("identity", &SPARSE_IDENTITY)
            .finish_non_exhaustive()
    }
}

impl SparseEncoder {
    /// Verify every pinned file (size, SHA-256), then load. Order: the files → `config.json`
    /// parsed and asserted → the tokenizer → the weights through `load_path` → the model.
    /// Nothing is parsed before its bytes are verified.
    ///
    /// # Errors
    ///
    /// `Error::Model` naming the encoder, the file and both values on a mismatch, or what failed
    /// to load.
    pub fn load(dir: &Path, load_path: LoadPath) -> Result<Self> {
        crate::model::verify_files_sparse(dir)?;
        assert_config(dir)?;
        let mut tokenizer = load_tokenizer(&dir.join(PINNED_SPARSE.files[1].name), sparse_err)?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: PINNED_SPARSE.max_tokens,
                ..TruncationParams::default()
            }))
            .map_err(|e| sparse_err(format!("cannot set truncation: {e}")))?;
        tokenizer.with_padding(None);
        let special = special_ids(&tokenizer);

        let weights = bytes::read(&dir.join(PINNED_SPARSE.files[2].name), load_path)?;
        let device = Device::Cpu;
        let vb = VarBuilder::from_slice_safetensors(weights.as_slice(), DType::F32, &device)
            .map_err(|e| sparse_err(format!("cannot load weights: {e}")))?;
        let build = || -> candle_core::Result<(Embeddings, Vec<Block>, Head)> {
            let word = vb.get(
                (VOCABULARY, DIM),
                "distilbert.embeddings.word_embeddings.weight",
            )?;
            let embeddings = Embeddings::load(vb.pp("distilbert.embeddings"), word.clone())?;
            let blocks = (0..LAYERS)
                .map(|i| Block::load(vb.pp(format!("distilbert.transformer.layer.{i}"))))
                .collect::<candle_core::Result<Vec<_>>>()?;
            let head = Head::load(&vb, word)?;
            Ok((embeddings, blocks, head))
        };
        let (embeddings, blocks, head) =
            build().map_err(|e| sparse_err(format!("cannot build model: {e}")))?;
        // `weights` is dropped here: candle copied every tensor into its own storage.

        Ok(Self {
            tokenizer,
            special,
            embeddings,
            blocks,
            head,
            device,
        })
    }

    /// One document, alone, truncated to 512 tokens: its kept `(token id, weight)` entries,
    /// ascending by id, weights above zero, special tokens excluded (research D3).
    ///
    /// # Errors
    ///
    /// `Error::Model` if tokenisation or the forward pass fails.
    pub fn encode(&self, text: &str) -> Result<Expansion> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| sparse_err(format!("cannot tokenise: {e}")))?;
        // Stride 0: whatever ran past the window is the overflow, so any overflow is truncation.
        let truncated = !encoding.get_overflowing().is_empty();
        let maxima = self
            .max_logits(encoding.get_ids())
            .map_err(|e| sparse_err(format!("forward pass failed: {e}")))?;

        let mut entries = Vec::new();
        for (id, &logit) in (0u32..).zip(&maxima) {
            if self.special.contains(&id) {
                continue;
            }
            // `log1p` twice, in f64 on the host: 30 522 values, and no weight collapses to
            // zero where `ln(1 + x)` in f32 would round `1 + x` to 1.
            let weight = f64::from(logit.max(0.0)).ln_1p().ln_1p() as f32;
            if weight > 0.0 {
                entries.push((id, weight));
            }
        }
        Ok(Expansion { entries, truncated })
    }

    /// The token ids the encoder sees for `text` (special tokens added, truncated to the window),
    /// for the tokenization-parity test.
    ///
    /// # Errors
    ///
    /// `Error::Model` if tokenisation fails.
    pub fn token_ids(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| sparse_err(format!("cannot tokenise: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    /// The identity recorded in a sparse index (data-model `SparseRecord.encoder`).
    #[must_use]
    pub fn identity(&self) -> &str {
        SPARSE_IDENTITY
    }

    /// The masked-LM logits' maximum over positions, per vocabulary entry.
    fn max_logits(&self, ids: &[u32]) -> candle_core::Result<Vec<f32>> {
        let input = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let mut hidden = self.embeddings.forward(&input)?;
        for block in &self.blocks {
            hidden = block.forward(&hidden)?;
        }
        let logits = self.head.forward(&hidden)?; // [1, tokens, vocabulary]
        logits.max(1)?.squeeze(0)?.to_vec1::<f32>()
    }
}

/// Word and position embeddings, summed and normalised.
struct Embeddings {
    word: Embedding,
    position: Embedding,
    norm: LayerNorm,
}

impl Embeddings {
    fn load(vb: VarBuilder, word: Tensor) -> candle_core::Result<Self> {
        Ok(Self {
            word: Embedding::new(word, DIM),
            position: candle_nn::embedding(
                PINNED_SPARSE.max_tokens,
                DIM,
                vb.pp("position_embeddings"),
            )?,
            norm: candle_nn::layer_norm(DIM, LAYER_NORM_EPS, vb.pp("LayerNorm"))?,
        })
    }

    fn forward(&self, ids: &Tensor) -> candle_core::Result<Tensor> {
        let (_, tokens) = ids.dims2()?;
        let positions = Tensor::arange(0u32, tokens as u32, ids.device())?;
        let summed = self
            .word
            .forward(ids)?
            .broadcast_add(&self.position.forward(&positions)?)?;
        self.norm.forward(&summed)
    }
}

/// One transformer block: self-attention, residual and norm; feed-forward, residual and norm.
struct Block {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    attention_norm: LayerNorm,
    up: Linear,
    down: Linear,
    output_norm: LayerNorm,
}

impl Block {
    fn load(vb: VarBuilder) -> candle_core::Result<Self> {
        let attention = vb.pp("attention");
        Ok(Self {
            q: candle_nn::linear(DIM, DIM, attention.pp("q_lin"))?,
            k: candle_nn::linear(DIM, DIM, attention.pp("k_lin"))?,
            v: candle_nn::linear(DIM, DIM, attention.pp("v_lin"))?,
            out: candle_nn::linear(DIM, DIM, attention.pp("out_lin"))?,
            attention_norm: candle_nn::layer_norm(DIM, LAYER_NORM_EPS, vb.pp("sa_layer_norm"))?,
            up: candle_nn::linear(DIM, HIDDEN, vb.pp("ffn.lin1"))?,
            down: candle_nn::linear(HIDDEN, DIM, vb.pp("ffn.lin2"))?,
            output_norm: candle_nn::layer_norm(DIM, LAYER_NORM_EPS, vb.pp("output_layer_norm"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let (batch, tokens, _) = x.dims3()?;
        let per_head = DIM / HEADS;
        let split = |t: Tensor| {
            t.reshape((batch, tokens, HEADS, per_head))?
                .transpose(1, 2)?
                .contiguous()
        };
        let q = (split(self.q.forward(x)?)? / (per_head as f64).sqrt())?;
        let k = split(self.k.forward(x)?)?;
        let v = split(self.v.forward(x)?)?;
        // One document at its own length: no padding, so nothing to mask.
        let scores = q.matmul(&k.t()?.contiguous()?)?;
        let weights = candle_nn::ops::softmax(&scores, D::Minus1)?;
        let context = weights
            .matmul(&v)?
            .transpose(1, 2)?
            .reshape((batch, tokens, DIM))?
            .contiguous()?;
        let attended = self
            .attention_norm
            .forward(&(self.out.forward(&context)? + x)?)?;
        let fed = self
            .down
            .forward(&self.up.forward(&attended)?.gelu_erf()?)?;
        self.output_norm.forward(&(fed + attended)?)
    }
}

/// The masked-LM head: transform, exact GELU, norm, and the projector tied to the word
/// embeddings (`vocab_projector` carries only its bias).
struct Head {
    transform: Linear,
    norm: LayerNorm,
    projector: Linear,
}

impl Head {
    fn load(vb: &VarBuilder, word: Tensor) -> candle_core::Result<Self> {
        Ok(Self {
            transform: candle_nn::linear(DIM, DIM, vb.pp("vocab_transform"))?,
            norm: candle_nn::layer_norm(DIM, LAYER_NORM_EPS, vb.pp("vocab_layer_norm"))?,
            projector: Linear::new(word, Some(vb.get(VOCABULARY, "vocab_projector.bias")?)),
        })
    }

    fn forward(&self, hidden: &Tensor) -> candle_core::Result<Tensor> {
        let transformed = self.transform.forward(hidden)?.gelu_erf()?;
        self.projector.forward(&self.norm.forward(&transformed)?)
    }
}

/// Assert `config.json` against the architecture the forward pass is written for.
fn assert_config(dir: &Path) -> Result<()> {
    let path = dir.join(PINNED_SPARSE.files[0].name);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| sparse_err(format!("cannot read {}: {e}", path.display())))?;
    let config: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| sparse_err(format!("cannot parse {}: {e}", path.display())))?;
    let expected = [
        ("model_type", serde_json::json!("distilbert")),
        ("dim", serde_json::json!(DIM)),
        ("n_layers", serde_json::json!(LAYERS)),
        ("n_heads", serde_json::json!(HEADS)),
        ("hidden_dim", serde_json::json!(HIDDEN)),
        ("vocab_size", serde_json::json!(VOCABULARY)),
        (
            "max_position_embeddings",
            serde_json::json!(PINNED_SPARSE.max_tokens),
        ),
        ("activation", serde_json::json!("gelu")),
        ("sinusoidal_pos_embds", serde_json::json!(false)),
    ];
    for (name, want) in expected {
        let have = config.get(name).unwrap_or(&serde_json::Value::Null);
        if *have != want {
            return Err(sparse_err(format!(
                "config.json {name} is {have}, expected {want}"
            )));
        }
    }
    Ok(())
}

/// A tokenizer from verified bytes; `err` names whose file it is.
fn load_tokenizer(path: &Path, err: fn(String) -> xtriever_core::Error) -> Result<Tokenizer> {
    let bytes =
        std::fs::read(path).map_err(|e| err(format!("cannot read {}: {e}", path.display())))?;
    Tokenizer::from_bytes(&bytes).map_err(|e| err(format!("cannot load {}: {e}", path.display())))
}

/// The ids of [`SPECIAL_TOKENS`] the tokenizer knows, ascending.
fn special_ids(tokenizer: &Tokenizer) -> Vec<u32> {
    let ids: BTreeSet<u32> = SPECIAL_TOKENS
        .iter()
        .filter_map(|t| tokenizer.token_to_id(t))
        .collect();
    ids.into_iter().collect()
}

/// The query side — every installation that searches a sparse index.
pub struct SparseQuery {
    tokenizer: Tokenizer,
    special: Vec<u32>,
    /// Indexed by token id: whether the query-side table gives it a positive entry.
    positive: Vec<bool>,
}

impl std::fmt::Debug for SparseQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparseQuery")
            .field("special", &self.special)
            .finish_non_exhaustive()
    }
}

impl SparseQuery {
    /// From a tokenizer and a query-side table, each verified against an expected SHA-256
    /// before it is parsed — they are an index's own files, so a difference is corruption.
    ///
    /// # Errors
    ///
    /// `Error::Corrupt` naming the file and both hashes on a mismatch, or naming the file if it
    /// cannot be read or parsed, or naming a table token the tokenizer does not know.
    pub fn open(
        tokenizer: &Path,
        table: &Path,
        tokenizer_sha256: &str,
        table_sha256: &str,
    ) -> Result<Self> {
        let tokenizer_bytes = read_verified(tokenizer, tokenizer_sha256)?;
        let table_bytes = read_verified(table, table_sha256)?;

        let mut parsed = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| corrupt(format!("cannot load {}: {e}", tokenizer.display())))?;
        parsed
            .with_truncation(None)
            .map_err(|e| corrupt(format!("cannot clear truncation: {e}")))?;
        parsed.with_padding(None);

        let entries: std::collections::BTreeMap<String, f64> = serde_json::from_slice(&table_bytes)
            .map_err(|e| corrupt(format!("cannot parse {}: {e}", table.display())))?;
        let mut positive = vec![false; parsed.get_vocab_size(true)];
        for (token, weight) in entries {
            let id = parsed.token_to_id(&token).ok_or_else(|| {
                corrupt(format!(
                    "{} names {token:?}, which {} does not know",
                    table.display(),
                    tokenizer.display()
                ))
            })?;
            if let Some(slot) = positive.get_mut(id as usize) {
                *slot = weight > 0.0;
            }
        }
        let special = special_ids(&parsed);
        Ok(Self {
            tokenizer: parsed,
            special,
            positive,
        })
    }

    /// The query's distinct kept token ids, ascending (data-model `QueryTerms`): tokenised
    /// without truncation or added special tokens, special tokens excluded, kept if the table's
    /// entry is positive.
    ///
    /// # Errors
    ///
    /// `Error::Model` naming the encoder if tokenisation fails.
    pub fn terms(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| sparse_err(format!("cannot tokenise the query: {e}")))?;
        let kept: BTreeSet<u32> = encoding
            .get_ids()
            .iter()
            .copied()
            .filter(|id| !self.special.contains(id))
            .filter(|&id| self.positive.get(id as usize).copied().unwrap_or(false))
            .collect();
        Ok(kept.into_iter().collect())
    }
}

/// Read `path` whole and check its SHA-256 against `expected` before anything parses it.
fn read_verified(path: &Path, expected: &str) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};

    let bytes =
        std::fs::read(path).map_err(|e| corrupt(format!("cannot read {}: {e}", path.display())))?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != expected {
        return Err(corrupt(format!(
            "{} has sha256 {digest}, expected {expected}",
            path.display()
        )));
    }
    Ok(bytes)
}

/// The `_sparse` field value for an expansion (research D4): each entry's term `s<id>` repeated
/// `round(weight × scale)` times — computed in f64, halves rounded away from zero — entries of
/// zero occurrences dropped, terms in the entries' (ascending) order, separated by single spaces.
/// The empty expansion is the empty string.
#[must_use]
pub fn field_text(expansion: &Expansion, scale: u32) -> String {
    let mut out = String::new();
    for &(id, weight) in &expansion.entries {
        let occurrences = (f64::from(weight) * f64::from(scale)).round();
        // An encoder's weights are finite and positive; a caller-built expansion may not be.
        if !occurrences.is_finite() || occurrences < 1.0 {
            continue;
        }
        for _ in 0..occurrences as u64 {
            if !out.is_empty() {
                out.push(' ');
            }
            // Writing to a String cannot fail.
            let _ = write!(out, "s{id}");
        }
    }
    out
}
