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
use std::path::{Path, PathBuf};

use candle_core::{D, DType, Device, Module, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, VarBuilder};
use tokenizers::{Tokenizer, TruncationParams};
use xtriever_core::Result;

use crate::LoadPath;
use crate::error::{corrupt, schema_err, sparse_err};
use crate::model::{
    PINNED_SPARSE, SPARSE_IDENTITY, check_sha256, read_pinned, sha256_hex, verify_file,
};

/// The configuration the forward pass is written for; `config.json` must say exactly this.
const DIM: usize = 768;
const LAYERS: usize = 6;
const HEADS: usize = 12;
const HIDDEN: usize = 3072;
const VOCABULARY: usize = 30_522;
/// DistilBERT's layer-norm epsilon, fixed in the architecture rather than the configuration.
const LAYER_NORM_EPS: f64 = 1e-12;

/// The query side's file names inside a sparse index's `sparse/` directory (research D6).
pub const QUERY_TOKENIZER: &str = "tokenizer.json";
/// The encoder's `idf.json`, under the name that says what it is to an index.
pub const QUERY_TABLE: &str = "query-table.json";

/// The largest `scale` a sparse index accepts: at the largest weight the encoder can produce
/// ([`MAX_WEIGHT`]) an entry is then at most 4,500 occurrences. The spike measured 10 and 100.
pub const MAX_SCALE: u32 = 1_000;

/// The largest weight an expansion may carry. The encoder's weight is `ln(1 + ln(1 + x))` of an
/// `f32` logit `x`, which is at most `ln(1 + ln(1 + f32::MAX))` ≈ 4.4967 — so a larger weight
/// did not come from the encoder, and [`field_text`] refuses it rather than write thousands of
/// occurrences (a test checks the bound).
pub const MAX_WEIGHT: f32 = 4.5;

/// The tokens whose weights the recipe zeroes and the query side never keeps.
const SPECIAL_TOKENS: [&str; 5] = ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]"];

/// Their ids in the pinned tokenizer, ascending: `[PAD]` 0, `[UNK]` 100, `[CLS]` 101, `[SEP]`
/// 102, `[MASK]` 103. The encoder refuses a tokenizer that disagrees, and the fixture records
/// the same ids (`tests/sparse_rules.rs`), so [`Expansion::validate`] can check them without one.
pub const SPECIAL_IDS: [u32; 5] = [0, 100, 101, 102, 103];

/// The encoder's vocabulary: every token id is below this.
pub const VOCABULARY_SIZE: u32 = 30_522;

/// One document's expansion: its kept `(token id, weight)` entries, ascending by id, every
/// weight above zero, special tokens absent (research D3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Expansion {
    /// `(token id, weight)`, ascending by id.
    pub entries: Vec<(u32, f32)>,
    /// The document ran past the encoder's window and was truncated to it.
    pub truncated: bool,
}

impl Expansion {
    /// Check the shape the encoder produces and an index can store: every id in the
    /// vocabulary and not a special token, ids strictly ascending (so none repeats), every
    /// weight finite, above zero and at most [`MAX_WEIGHT`]. An expansion from elsewhere — a
    /// cache, a caller — is refused, never repaired.
    ///
    /// # Errors
    ///
    /// `Error::Schema` naming the first offending entry.
    pub fn validate(&self) -> Result<()> {
        for (i, &(id, weight)) in self.entries.iter().enumerate() {
            if id >= VOCABULARY_SIZE || SPECIAL_IDS.contains(&id) {
                return Err(schema_err(format!(
                    "expansion entry {i} names token {id}, which is {}",
                    if id >= VOCABULARY_SIZE {
                        "outside the encoder's vocabulary"
                    } else {
                        "a special token"
                    }
                )));
            }
            if !(weight.is_finite() && weight > 0.0 && weight <= MAX_WEIGHT) {
                return Err(schema_err(format!(
                    "expansion entry {i} (token {id}) has weight {weight}; weights must be \
                     finite, above zero and at most {MAX_WEIGHT}"
                )));
            }
        }
        for (i, pair) in self.entries.windows(2).enumerate() {
            let (previous, id) = (pair[0].0, pair[1].0);
            if previous >= id {
                return Err(schema_err(format!(
                    "expansion entry {} (token {id}) follows token {previous}; ids must be \
                     strictly ascending",
                    i + 1
                )));
            }
        }
        Ok(())
    }
}

/// The pinned document encoder — build host only.
pub struct SparseEncoder {
    /// The directory it was loaded from, for [`write_query_side`](Self::write_query_side).
    dir: PathBuf,
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
    /// Verify every pinned file (size, SHA-256), then load. Each file is read once — the
    /// weights through `load_path` — and the bytes read are the bytes checked and then parsed:
    /// `config.json` asserted, the tokenizer built, the model built. Nothing is parsed before
    /// every file is verified.
    ///
    /// # Errors
    ///
    /// `Error::Model` naming the encoder, the file and both values on a mismatch, or what failed
    /// to load.
    pub fn load(dir: &Path, load_path: LoadPath) -> Result<Self> {
        let [config, tokenizer, weights, table] = &PINNED_SPARSE.files;
        // Every file this loader parses is read once and its bytes checked before any of them
        // is parsed. The table is not parsed here (the query side reads an index's copy), so
        // it is verified by a streamed hash and never held. A buffered load holds the weights
        // twice while candle copies them, as every model in the engine does; `LoadPath::Mmap`
        // is the path that avoids it.
        let config = read_pinned(dir, config, LoadPath::Buffered, sparse_err)?;
        let tokenizer_bytes = read_pinned(dir, tokenizer, LoadPath::Buffered, sparse_err)?;
        let weights = read_pinned(dir, weights, load_path, sparse_err)?;
        verify_file(dir, table, sparse_err)?;

        assert_config(config.as_slice())?;
        let mut tokenizer = Tokenizer::from_bytes(tokenizer_bytes.as_slice())
            .map_err(|e| sparse_err(format!("cannot load {}: {e}", PINNED_SPARSE.files[1].name)))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: PINNED_SPARSE.max_tokens,
                ..TruncationParams::default()
            }))
            .map_err(|e| sparse_err(format!("cannot set truncation: {e}")))?;
        tokenizer.with_padding(None);
        let special = special_ids(&tokenizer);
        if special != SPECIAL_IDS {
            return Err(sparse_err(format!(
                "tokenizer.json maps the special tokens to {special:?}, expected {SPECIAL_IDS:?}"
            )));
        }

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
            dir: dir.to_path_buf(),
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
        let (ids, truncated) = self.tokenise(text)?;
        let maxima = self
            .max_logits(&ids)
            .map_err(|e| sparse_err(format!("forward pass failed: {e}")))?;

        let mut entries = Vec::new();
        for (id, &logit) in (0u32..).zip(&maxima) {
            // A broken forward pass must not pass for a document with nothing to expand.
            if !logit.is_finite() {
                return Err(sparse_err(format!(
                    "the forward pass produced a non-finite logit ({logit}) for token {id}"
                )));
            }
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
        Ok(self.tokenise(text)?.0)
    }

    /// The one tokenisation `encode` and `token_ids` share: the window's ids
    /// (special tokens added, truncated to 512) and whether the text ran past it.
    fn tokenise(&self, text: &str) -> Result<(Vec<u32>, bool)> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| sparse_err(format!("cannot tokenise: {e}")))?;
        // Stride 0: whatever ran past the window is the overflow, so any overflow is truncation.
        let truncated = !encoding.get_overflowing().is_empty();
        Ok((encoding.get_ids().to_vec(), truncated))
    }

    /// Copy the query side — the pinned `tokenizer.json`, and `idf.json` as `query-table.json` —
    /// into `dest` (created if absent), each re-read from the encoder's directory and checked
    /// against its pin before it is written. Returns the two files' SHA-256, as a sparse index
    /// records them (research D6). Each file is written atomically and the directory synced.
    ///
    /// # Errors
    ///
    /// `Error::Model` for a file that fails its pin; `Error::Io` for the writes.
    pub fn write_query_side(&self, dest: &Path) -> Result<(String, String)> {
        std::fs::create_dir_all(dest)?;
        // Destructured, as in `load`: a change in the pin list's shape is a compile error here,
        // never the wrong file copied under the right name.
        let [_, tokenizer, _, table] = &PINNED_SPARSE.files;
        for (pin, name) in [(tokenizer, QUERY_TOKENIZER), (table, QUERY_TABLE)] {
            // Re-read and re-checked: the directory may have changed since `load`.
            let bytes = read_pinned(&self.dir, pin, LoadPath::Buffered, sparse_err)?;
            let path = dest.join(name);
            xtriever_core::fs::write_atomically(
                &path,
                &dest.join(format!("{name}.tmp")),
                bytes.as_slice(),
            )?;
        }
        xtriever_core::fs::sync_dir(dest)?;
        Ok((tokenizer.sha256.to_owned(), table.sha256.to_owned()))
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
fn assert_config(bytes: &[u8]) -> Result<()> {
    let config: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| sparse_err(format!("cannot parse config.json: {e}")))?;
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
    let bytes =
        std::fs::read(path).map_err(|e| corrupt(format!("cannot read {}: {e}", path.display())))?;
    check_sha256(path, &sha256_hex(&bytes), expected, corrupt)?;
    Ok(bytes)
}

/// The scale rule, owned here with [`field_text`], which applies it: `1..=`[`MAX_SCALE`]. A
/// sparse index checks its option against it at creation and at open, so an index that
/// validates can always write its expansions.
///
/// # Errors
///
/// `Error::Schema` naming the scale.
pub fn validate_scale(scale: u32) -> Result<()> {
    if (1..=MAX_SCALE).contains(&scale) {
        Ok(())
    } else {
        Err(schema_err(format!(
            "sparse scale {scale} is outside 1..={MAX_SCALE}"
        )))
    }
}

/// The `_sparse` field value for an expansion (research D4): each entry's term `s<id>` repeated
/// `round(weight × scale)` times — computed in f64, halves rounded away from zero — entries of
/// zero occurrences dropped, terms in ascending id order, separated by single spaces. The empty
/// expansion is the empty string. This is the one rule the pipeline and the evaluation harness
/// both use.
///
/// # Errors
///
/// `Error::Schema` if `scale` is outside `1..=`[`MAX_SCALE`] or the expansion fails
/// [`Expansion::validate`] — so no input can make the text unboundedly long.
pub fn field_text(expansion: &Expansion, scale: u32) -> Result<String> {
    validate_scale(scale)?;
    expansion.validate()?;
    let mut out = String::new();
    for &(id, weight) in &expansion.entries {
        // Validated: finite and at most MAX_WEIGHT × MAX_SCALE, so the cast is exact.
        let occurrences = (f64::from(weight) * f64::from(scale)).round() as u64;
        for _ in 0..occurrences {
            if !out.is_empty() {
                out.push(' ');
            }
            // Writing to a String cannot fail.
            let _ = write!(out, "s{id}");
        }
    }
    Ok(out)
}
