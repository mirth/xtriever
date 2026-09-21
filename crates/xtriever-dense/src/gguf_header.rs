//! What a GGUF header declares, read with the pinned engine's own reader (Feature 026,
//! research D5): the metadata the pins are asserted against and the tensor table.
//!
//! This file is byte-identical in `xtriever-dense` and `xtriever-rerank` — the downward
//! dependency rule (Principle V) gives the two stage crates no shared home below `core`, and
//! `core` stays engine-free — and `tests/twins.rs` in the dense crate fails if the two copies
//! ever differ, so a fix lands in both or in neither.

use candle_core::quantized::GgmlDType;
use candle_core::quantized::gguf_file::{Content, Value};

use crate::error::model_err;

pub(crate) struct GgufHeader {
    content: Content,
}

/// What a pinned BERT artefact's header must declare: the part of each stage crate's pin that
/// the two crates assert alike. Each crate builds one from its own `PINNED_Q8`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BertPin {
    pub architecture: &'static str,
    pub blocks: usize,
    pub embedding_length: usize,
    pub heads: usize,
    pub feed_forward_length: usize,
    pub context_length: usize,
    /// Tensors the file must carry by name (the re-ranker's classification head; none for the
    /// embedder), checked after the shape and before the count so a file that is the right
    /// model without its head is refused for the head.
    pub required_tensors: &'static [&'static str],
    pub quantisation: &'static str,
    pub quantised_tensors: usize,
}

impl GgufHeader {
    pub fn read(bytes: &[u8]) -> xtriever_core::Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes);
        let content = Content::read(&mut cursor)
            .map_err(|e| model_err(format!("not a GGUF file this engine can read: {e}")))?;
        Ok(Self { content })
    }

    pub fn string(&self, key: &str) -> xtriever_core::Result<&str> {
        self.content
            .metadata
            .get(key)
            .ok_or_else(|| model_err(format!("GGUF header declares no {key}")))?
            .to_string()
            .map(String::as_str)
            .map_err(|e| model_err(format!("GGUF header {key}: {e}")))
    }

    pub fn number(&self, key: &str) -> xtriever_core::Result<usize> {
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

    /// `key` must equal `expected`; the message names the field and both values.
    pub fn expect_number(&self, key: &str, expected: usize) -> xtriever_core::Result<()> {
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

    /// Assert the header against a pin: the architecture, block count, embedding length, head
    /// count, feed-forward length, context length (exactly: the encoder sizes its position
    /// table from it), the tensors required by name and the number of eight-bit tensors. The
    /// bytes were verified against the
    /// pin's hash before; this guards the pin itself — a pinned file that is not the model the
    /// forward pass is written for is refused by name, not run.
    pub fn assert_bert(&self, pin: &BertPin) -> xtriever_core::Result<()> {
        let architecture = self.string("general.architecture")?;
        if architecture != pin.architecture {
            return Err(model_err(format!(
                "GGUF header architecture is {architecture:?}, expected {:?}",
                pin.architecture
            )));
        }
        let prefix = pin.architecture;
        self.expect_number(&format!("{prefix}.block_count"), pin.blocks)?;
        self.expect_number(&format!("{prefix}.embedding_length"), pin.embedding_length)?;
        self.expect_number(&format!("{prefix}.attention.head_count"), pin.heads)?;
        self.expect_number(
            &format!("{prefix}.feed_forward_length"),
            pin.feed_forward_length,
        )?;
        self.expect_number(&format!("{prefix}.context_length"), pin.context_length)?;
        for tensor in pin.required_tensors {
            if !self.has_tensor(tensor) {
                return Err(model_err(format!(
                    "GGUF file has no {tensor}: not a cross-encoder with its classification head"
                )));
            }
        }
        let quantised = self.quantised_tensors();
        if quantised != pin.quantised_tensors {
            return Err(model_err(format!(
                "GGUF file holds {quantised} {} tensors, expected {} — not the pinned artefact",
                pin.quantisation, pin.quantised_tensors
            )));
        }
        Ok(())
    }

    /// A file that declares a layer-norm epsilon must declare the pinned configuration's
    /// (compared as `f32`, the width the file stores); a file that declares none uses the
    /// configuration's. Refused naming both, like every other pinned field.
    pub fn assert_layer_norm_epsilon(
        &self,
        architecture: &str,
        configured: f64,
    ) -> xtriever_core::Result<()> {
        if let Some(declared) = self.layer_norm_epsilon(architecture)?
            && declared != configured as f32
        {
            return Err(model_err(format!(
                "GGUF header layer_norm_epsilon is {declared:e}, config.json says {configured:e}"
            )));
        }
        Ok(())
    }

    /// The layer-norm epsilon the file declares, if it declares one; an error if it declares
    /// something that is not a float.
    fn layer_norm_epsilon(&self, architecture: &str) -> xtriever_core::Result<Option<f32>> {
        let key = format!("{architecture}.attention.layer_norm_epsilon");
        match self.content.metadata.get(&key) {
            None => Ok(None),
            Some(value) => value
                .to_f32()
                .map(Some)
                .map_err(|e| model_err(format!("GGUF header {key}: {e}"))),
        }
    }

    pub fn quantised_tensors(&self) -> usize {
        self.content
            .tensor_infos
            .values()
            .filter(|t| t.ggml_dtype == GgmlDType::Q8_0)
            .count()
    }

    fn has_tensor(&self, name: &str) -> bool {
        self.content.tensor_infos.contains_key(name)
    }
}
