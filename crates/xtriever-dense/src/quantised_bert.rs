//! A BERT encoder over the eight-bit GGUF tensors the owner pinned (Feature 026, research D5;
//! ADR-0015).
//!
//! candle 0.9.2 ships a float `BertModel` and twenty-two quantised *decoder* models, but no
//! quantised BERT, so this is the float model's forward pass
//! (`candle_transformers::models::bert`, read line by line on 2026-09-21) written over the
//! file's tensors. Tensor names are the GGUF convention for `bert` (`token_embd`,
//! `blk.N.attn_q`, `blk.N.ffn_up`, …); the norms, biases, token-type and position tables are
//! dequantised once at load (they are float in the file anyway).
//!
//! **The arithmetic is f16 over the eight-bit-rounded weights** (owner's decision, 2026-09-21;
//! the fingerprint names it, `compute=f16`, from the same literal as the pins in `model.rs`). Every weight matrix is expanded from its eight-bit blocks to `f16` once at
//! load — the values stay exactly what the artefact fixed, the storage halves the float
//! model's — and each multiply converts the activations to `f16`, runs the float kernel, and
//! converts back. Measured on SciFact before the choice: candle's eight-bit CPU kernel (built
//! for one token at a time, fed 256) took 442 ms per embedding against 120 ms for this path and
//! 125 ms for the float artefact, with nDCG@10 0.64646 / 0.64642 / 0.64631 for the eight-bit,
//! f16 and f32 arithmetic — the weight rounding, not the arithmetic, is what the artefact
//! changes. The mode is fixed here in code: candle's `QMatMul::from_arc` reads it from two
//! environment variables, and an environment variable must not be able to change a vector,
//! so the matmul is constructed explicitly and `from_arc` is never called.
//!
//! The arithmetic is the reference's: embeddings summed then normalised; per block, scaled
//! dot-product attention with the extended mask (`(1 − mask) × f32::MIN`), a residual and a
//! norm, then the erf GELU feed-forward, a residual and a norm. Nothing is batched across
//! texts, as in the float path.

use candle_core::quantized::QMatMul;
use candle_core::{D, DType, Device, Module, Result, Tensor};
use candle_nn::LayerNorm;
use candle_transformers::quantized_nn::{Embedding, layer_norm};
use candle_transformers::quantized_var_builder::VarBuilder;

/// A linear layer whose matrix is the artefact's eight-bit tensor expanded to `f16` at load,
/// constructed explicitly so candle's environment switches cannot change the mode.
struct Linear {
    weight: QMatMul,
    bias: Tensor,
}

impl Linear {
    fn new(vb: &VarBuilder, in_dim: usize, out_dim: usize) -> Result<Self> {
        let weight = vb.get((out_dim, in_dim), "weight")?;
        let weight = QMatMul::TensorF16(weight.dequantize_f16(vb.device())?);
        let bias = vb.get(out_dim, "bias")?.dequantize(vb.device())?;
        Ok(Self { weight, bias })
    }
}

impl Module for Linear {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        self.weight.forward(xs)?.broadcast_add(&self.bias)
    }
}

/// The shape the encoder is built for: what the header declared and the pin asserted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    pub blocks: usize,
    pub heads: usize,
    pub hidden: usize,
    pub feed_forward: usize,
    pub context_length: usize,
    pub layer_norm_eps: f64,
}

struct Block {
    query: Linear,
    key: Linear,
    value: Linear,
    output: Linear,
    attention_norm: LayerNorm,
    up: Linear,
    down: Linear,
    output_norm: LayerNorm,
}

pub(crate) struct QuantisedBert {
    word: Embedding,
    position: Embedding,
    token_type: Embedding,
    embedding_norm: LayerNorm,
    blocks: Vec<Block>,
    heads: usize,
    head_dim: usize,
}

impl QuantisedBert {
    /// Build from a whole GGUF file's bytes (the header was asserted against the pin first).
    pub fn from_gguf(bytes: &[u8], shape: Shape, device: &Device) -> Result<Self> {
        let vb = VarBuilder::from_gguf_buffer(bytes, device)?;
        let vocabulary = vb.get_no_shape("token_embd.weight")?.shape().dims()[0];
        let word = Embedding::new(vocabulary, shape.hidden, vb.pp("token_embd"))?;
        let position = Embedding::new(shape.context_length, shape.hidden, vb.pp("position_embd"))?;
        let token_type = Embedding::new(2, shape.hidden, vb.pp("token_types"))?;
        let embedding_norm =
            layer_norm(shape.hidden, shape.layer_norm_eps, vb.pp("token_embd_norm"))?;
        let blocks = (0..shape.blocks)
            .map(|i| {
                let b = vb.pp(format!("blk.{i}"));
                Ok(Block {
                    query: Linear::new(&b.pp("attn_q"), shape.hidden, shape.hidden)?,
                    key: Linear::new(&b.pp("attn_k"), shape.hidden, shape.hidden)?,
                    value: Linear::new(&b.pp("attn_v"), shape.hidden, shape.hidden)?,
                    output: Linear::new(&b.pp("attn_output"), shape.hidden, shape.hidden)?,
                    attention_norm: layer_norm(
                        shape.hidden,
                        shape.layer_norm_eps,
                        b.pp("attn_output_norm"),
                    )?,
                    up: Linear::new(&b.pp("ffn_up"), shape.hidden, shape.feed_forward)?,
                    down: Linear::new(&b.pp("ffn_down"), shape.feed_forward, shape.hidden)?,
                    output_norm: layer_norm(
                        shape.hidden,
                        shape.layer_norm_eps,
                        b.pp("layer_output_norm"),
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            word,
            position,
            token_type,
            embedding_norm,
            blocks,
            heads: shape.heads,
            head_dim: shape.hidden / shape.heads,
        })
    }

    /// The last hidden state, `(batch, seq, hidden)`; `attention_mask` all ones when `None`.
    pub fn forward(
        &self,
        ids: &Tensor,
        type_ids: &Tensor,
        attention_mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let (_batch, seq) = ids.dims2()?;
        let positions = Tensor::arange(0u32, seq as u32, ids.device())?;
        let mut hidden = (self.word.forward(ids)? + self.token_type.forward(type_ids)?)?
            .broadcast_add(&self.position.forward(&positions)?)?;
        hidden = self.embedding_norm.forward(&hidden)?;

        let mask = match attention_mask {
            Some(mask) => mask.clone(),
            None => ids.ones_like()?,
        };
        // The extended mask the reference adds to the scores: 0 where attended, f32::MIN where
        // padding, broadcast over batch, head and query position.
        let extended = (mask.ones_like()? - &mask)?
            .to_dtype(DType::F32)?
            .unsqueeze(1)?
            .unsqueeze(1)?
            .broadcast_mul(&Tensor::new(f32::MIN, ids.device())?)?;

        for block in &self.blocks {
            let attended = self.attention(block, &hidden, &extended)?;
            let attended = block.attention_norm.forward(&(attended + &hidden)?)?;
            let intermediate = block.up.forward(&attended)?.gelu_erf()?;
            hidden = block
                .output_norm
                .forward(&(block.down.forward(&intermediate)? + attended)?)?;
        }
        Ok(hidden)
    }

    fn attention(&self, block: &Block, hidden: &Tensor, extended_mask: &Tensor) -> Result<Tensor> {
        let (batch, seq, _) = hidden.dims3()?;
        let heads = |t: Tensor| -> Result<Tensor> {
            t.reshape((batch, seq, self.heads, self.head_dim))?
                .transpose(1, 2)?
                .contiguous()
        };
        let query = heads(block.query.forward(hidden)?)?;
        let key = heads(block.key.forward(hidden)?)?;
        let value = heads(block.value.forward(hidden)?)?;
        let scores = (query.matmul(&key.t()?)? / (self.head_dim as f64).sqrt())?
            .broadcast_add(extended_mask)?;
        let probabilities = candle_nn::ops::softmax(&scores, D::Minus1)?;
        let context = probabilities
            .matmul(&value)?
            .transpose(1, 2)?
            .contiguous()?
            .flatten_from(D::Minus2)?;
        block.output.forward(&context)
    }
}
